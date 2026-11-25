use sinclair_cpu_z80::{Bus, InterruptMode, Z80};
use std::collections::VecDeque;

struct TestBus {
    memory: Box<[u8; 65536]>,
    writes: Vec<(u16, u8)>,
    reads: Vec<u16>,
    vector: u8,
    acknowledgements: VecDeque<u8>,
    interrupt_operands: VecDeque<u8>,
    port_input: u8,
    port_reads: Vec<u16>,
    port_writes: Vec<(u16, u8)>,
    reti_count: usize,
}

#[test]
fn opcode_fetches_are_distinct_from_operand_and_data_reads() {
    struct FetchBus {
        memory: [u8; 65536],
        opcode_reads: Vec<u16>,
        data_reads: Vec<u16>,
    }
    impl Bus for FetchBus {
        fn read_memory(&mut self, address: u16) -> u8 {
            self.data_reads.push(address);
            self.memory[usize::from(address)]
        }
        fn read_opcode(&mut self, address: u16) -> u8 {
            self.opcode_reads.push(address);
            self.memory[usize::from(address)]
        }
        fn write_memory(&mut self, address: u16, value: u8) {
            self.memory[usize::from(address)] = value;
        }
        fn read_port(&mut self, _: u16) -> u8 {
            0xff
        }
        fn write_port(&mut self, _: u16, _: u8) {}
    }
    let mut bus = FetchBus {
        memory: [0; 65536],
        opcode_reads: Vec::new(),
        data_reads: Vec::new(),
    };
    // LD A,12; RLC A; DD CB 01 46 (BIT 0,(IX+1)); HALT.
    bus.memory[..10].copy_from_slice(&[0x3e, 0x12, 0xcb, 0x07, 0xdd, 0xcb, 0x01, 0x46, 0x76, 0]);
    let mut cpu = Z80::new();
    cpu.ix = 0x8000;
    for _ in 0..5 {
        cpu.step(&mut bus);
    }
    assert_eq!(bus.opcode_reads, [0, 2, 3, 4, 5, 8, 9]);
    assert_eq!(bus.data_reads, [1, 6, 7, 0x8001]);
}

impl TestBus {
    fn new(program: &[u8]) -> Self {
        let mut bus = Self {
            memory: Box::new([0; 65536]),
            writes: Vec::new(),
            reads: Vec::new(),
            vector: 0xff,
            acknowledgements: VecDeque::new(),
            interrupt_operands: VecDeque::new(),
            port_input: 0,
            port_reads: Vec::new(),
            port_writes: Vec::new(),
            reti_count: 0,
        };
        bus.memory[..program.len()].copy_from_slice(program);
        bus
    }
}

impl Bus for TestBus {
    fn read_memory(&mut self, address: u16) -> u8 {
        self.reads.push(address);
        self.memory[usize::from(address)]
    }

    fn write_memory(&mut self, address: u16, value: u8) {
        self.writes.push((address, value));
        self.memory[usize::from(address)] = value;
    }

    fn read_port(&mut self, port: u16) -> u8 {
        self.port_reads.push(port);
        self.port_input
    }

    fn write_port(&mut self, port: u16, value: u8) {
        self.port_writes.push((port, value));
    }

    fn interrupt_acknowledge(&mut self) -> u8 {
        self.acknowledgements.pop_front().unwrap_or(self.vector)
    }

    fn interrupt_data(&mut self, _address: u16) -> u8 {
        self.interrupt_operands
            .pop_front()
            .expect("missing IM0 operand")
    }

    fn reti(&mut self) {
        self.reti_count += 1;
    }
}

#[test]
fn cpu_state_preserves_interrupt_delay_pending_inputs_and_halt() {
    let mut bus = TestBus::new(&[0xfb, 0x00, 0x76]); // EI; NOP; HALT
    let mut cpu = Z80::new();
    cpu.sp = 0xff00;
    cpu.a = 0x91;
    cpu.set_hl_alt(0xabcd);
    cpu.ix = 0x9876;
    cpu.im = InterruptMode::Mode1;
    cpu.step(&mut bus);
    cpu.set_irq(true);
    let saved = cpu.state_bytes();
    let mut restored = Z80::from_state_bytes(&saved).unwrap();
    let mut other_bus = TestBus::new(&[0xfb, 0x00, 0x76]);
    for _ in 0..2 {
        assert_eq!(cpu.step(&mut bus), restored.step(&mut other_bus));
        assert_eq!(cpu.state_bytes(), restored.state_bytes());
    }
    assert_eq!(bus.memory[..], other_bus.memory[..]);

    cpu.request_nmi();
    let mut restored = Z80::from_state_bytes(&cpu.state_bytes()).unwrap();
    let mut other_bus = TestBus::new(&[]);
    assert_eq!(cpu.step(&mut bus), restored.step(&mut other_bus));
    assert_eq!(cpu.state_bytes(), restored.state_bytes());

    let mut halt_bus = TestBus::new(&[0x76]);
    let mut halted = Z80::new();
    halted.step(&mut halt_bus);
    let mut restored = Z80::from_state_bytes(&halted.state_bytes()).unwrap();
    let mut other_bus = TestBus::new(&[0x76]);
    assert!(restored.halted);
    assert_eq!(halted.step(&mut halt_bus), restored.step(&mut other_bus));
    assert_eq!(halted.state_bytes(), restored.state_bytes());

    let mut invalid = saved;
    invalid[26] = 2;
    assert!(Z80::from_state_bytes(&invalid).is_err());
    invalid = saved;
    invalid[32] = 3;
    assert!(Z80::from_state_bytes(&invalid).is_err());
}

#[test]
fn base_registers_arithmetic_and_alternate_sets() {
    let mut bus = TestBus::new(&[
        0x01, 0x34, 0x12, // LD BC,1234
        0x3e, 0x7f, // LD A,7F
        0xc6, 0x01, // ADD A,1
        0x08, // EX AF,AF'
        0xd9, // EXX
        0x76, // HALT
    ]);
    let mut cpu = Z80::new();
    assert_eq!(cpu.step(&mut bus), 10);
    assert_eq!(cpu.bc(), 0x1234);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.f, 0x94); // sign, half carry, signed overflow
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!(cpu.af(), 0);
    assert_eq!((cpu.a_alt, cpu.f_alt), (0x80, 0x94));
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!((cpu.b_alt, cpu.c_alt), (0x12, 0x34));
    assert_eq!(cpu.step(&mut bus), 4);
    assert!(cpu.halted);
    let pc = cpu.pc;
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!(cpu.pc, pc);
}

#[test]
fn cb_and_indexed_cb_update_memory_registers_and_timing() {
    let mut bus = TestBus::new(&[
        0x06, 0x81, // LD B,81
        0xcb, 0x00, // RLC B
        0xdd, 0x21, 0x00, 0x20, // LD IX,2000
        0xdd, 0xcb, 0xfe, 0x06, // RLC (IX-2)
        0xfd, 0xcb, 0x02, 0x46, // BIT 0,(IY+2)
    ]);
    bus.memory[0x1ffe] = 0x80;
    bus.memory[0x2802] = 0x01;
    let mut cpu = Z80::new();
    cpu.iy = 0x2800;
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.step(&mut bus), 8);
    assert_eq!((cpu.b, cpu.f), (0x03, 0x05));
    assert_eq!(cpu.step(&mut bus), 14);
    assert_eq!(cpu.ix, 0x2000);
    assert_eq!(cpu.step(&mut bus), 23);
    assert_eq!(bus.memory[0x1ffe], 1);
    assert_eq!(cpu.step(&mut bus), 20);
    assert_eq!(cpu.f & 0x7d, 0x39); // H, X/Y from address high, preserved carry
    assert_eq!(cpu.r & 0x7f, 9);
}

#[test]
fn ed_block_transfer_repeats_one_iteration_at_a_time() {
    let mut bus = TestBus::new(&[0xed, 0xb0]); // LDIR
    bus.memory[0x1000] = 0x12;
    bus.memory[0x1001] = 0x34;
    let mut cpu = Z80::new();
    cpu.set_hl(0x1000);
    cpu.set_de(0x2000);
    cpu.set_bc(2);
    assert_eq!(cpu.step(&mut bus), 21);
    assert_eq!((cpu.pc, cpu.bc()), (0, 1));
    assert_eq!(cpu.step(&mut bus), 16);
    assert_eq!((cpu.pc, cpu.bc()), (2, 0));
    assert_eq!(&bus.memory[0x2000..0x2002], &[0x12, 0x34]);
}

#[test]
fn ei_delays_irq_until_after_following_instruction() {
    let mut bus = TestBus::new(&[0xfb, 0x00, 0x00]); // EI; NOP; NOP
    let mut cpu = Z80::new();
    cpu.sp = 0x1000;
    cpu.im = InterruptMode::Mode1;
    cpu.set_irq(true);
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!(cpu.pc, 2);
    assert_eq!(cpu.step(&mut bus), 13);
    assert_eq!((cpu.pc, cpu.sp), (0x38, 0x0ffe));
    assert_eq!(&bus.memory[0x0ffe..0x1000], &[2, 0]);
    assert!(!cpu.iff1 && !cpu.iff2);
}

#[test]
fn interrupt_modes_zero_and_two_use_bus_vector() {
    let mut bus = TestBus::new(&[]);
    let mut cpu = Z80::new();
    cpu.pc = 0x1234;
    cpu.sp = 0x2000;
    cpu.iff1 = true;
    cpu.iff2 = true;
    cpu.set_irq(true);
    bus.vector = 0xcf; // RST 08
    assert_eq!(cpu.step(&mut bus), 13);
    assert_eq!(cpu.pc, 0x08);
    assert_eq!(&bus.memory[0x1ffe..0x2000], &[0x34, 0x12]);

    cpu.pc = 0x4567;
    cpu.sp = 0x3000;
    cpu.i = 0x20;
    cpu.im = InterruptMode::Mode2;
    cpu.iff1 = true;
    bus.vector = 0xff;
    bus.memory[0x20ff] = 0x78;
    bus.memory[0x2100] = 0x56;
    assert_eq!(cpu.step(&mut bus), 19);
    assert_eq!(cpu.pc, 0x5678);
    assert_eq!(&bus.memory[0x2ffe..0x3000], &[0x67, 0x45]);
}

#[test]
fn im0_multibyte_instruction_reads_device_and_keeps_pc() {
    let mut bus = TestBus::new(&[]);
    let mut cpu = Z80::new();
    cpu.pc = 0x1234;
    cpu.sp = 0x2000;
    cpu.iff1 = true;
    cpu.set_irq(true);
    bus.vector = 0xcd; // CALL supplied by the device
    bus.interrupt_operands.extend([0x78, 0x56]);
    assert_eq!(cpu.step(&mut bus), 19);
    assert_eq!(cpu.pc, 0x5678);
    assert_eq!(&bus.memory[0x1ffe..0x2000], &[0x34, 0x12]);
}

#[test]
fn im0_prefix_fetches_another_acknowledged_opcode() {
    let mut bus = TestBus::new(&[]);
    let mut cpu = Z80::new();
    cpu.pc = 0x1234;
    cpu.iff1 = true;
    cpu.set_irq(true);
    bus.acknowledgements.extend([0xfd, 0x21]); // LD IY,nn
    bus.interrupt_operands.extend([0x78, 0x56]);
    assert_eq!(cpu.step(&mut bus), 18);
    assert_eq!((cpu.pc, cpu.iy, cpu.r), (0x1234, 0x5678, 2));
}

#[test]
fn nmi_preserves_iff2_and_retn_restores_iff1() {
    let mut bus = TestBus::new(&[]);
    bus.memory[0x66] = 0xed;
    bus.memory[0x67] = 0x45; // RETN
    let mut cpu = Z80::new();
    cpu.pc = 0x1234;
    cpu.sp = 0x2000;
    cpu.iff1 = false;
    cpu.iff2 = true;
    cpu.request_nmi();
    assert_eq!(cpu.step(&mut bus), 11);
    assert!(!cpu.iff1 && cpu.iff2);
    assert_eq!(cpu.step(&mut bus), 14);
    assert_eq!(cpu.pc, 0x1234);
    assert!(cpu.iff1);
}

#[test]
fn ed_ports_use_full_bc_address_and_preserve_carry() {
    let mut bus = TestBus::new(&[0xed, 0x78, 0xed, 0x79]); // IN A,(C); OUT (C),A
    bus.port_input = 0x80;
    let mut cpu = Z80::new();
    cpu.set_bc(0x1234);
    cpu.f = 1;
    assert_eq!(cpu.step(&mut bus), 12);
    assert_eq!((cpu.a, cpu.f), (0x80, 0x81));
    assert_eq!(bus.port_reads, [0x1234]);
    assert_eq!(cpu.step(&mut bus), 12);
    assert_eq!(bus.port_writes, [(0x1234, 0x80)]);
}

#[test]
fn im2_pushes_pc_before_reading_vector() {
    let mut bus = TestBus::new(&[]);
    let mut cpu = Z80::new();
    cpu.pc = 0x1234;
    cpu.sp = 0x2100;
    cpu.i = 0x20;
    cpu.im = InterruptMode::Mode2;
    cpu.iff1 = true;
    cpu.set_irq(true);
    bus.vector = 0xfe;
    assert_eq!(cpu.step(&mut bus), 19);
    assert_eq!(cpu.pc, 0x1234); // vector overlaps the just-written stack
}

#[test]
fn last_index_prefix_wins_and_all_prefixes_take_time() {
    let mut bus = TestBus::new(&[
        0xdd, 0xfd, 0x21, 0x34, 0x12, // LD IY,1234 with redundant DD
    ]);
    let mut cpu = Z80::new();
    assert_eq!(cpu.step(&mut bus), 18);
    assert_eq!((cpu.ix, cpu.iy, cpu.r), (0, 0x1234, 3));
}
