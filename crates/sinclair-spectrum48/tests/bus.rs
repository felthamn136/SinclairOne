use sinclair_cpu_z80::{Bus, Z80};
use sinclair_spectrum48::{
    bus::{SpectrumBus, SpectrumIo},
    keyboard::{Key, KeyboardIo, KeyboardMatrix},
    memory::{Memory, ROM_SIZE},
};

#[derive(Default)]
struct TestIo {
    reads: Vec<u16>,
    writes: Vec<(u16, u8)>,
    acknowledgements: usize,
    nmis: usize,
    reti_count: usize,
    interrupt_address: Option<u16>,
}

impl SpectrumIo for TestIo {
    fn read_port(&mut self, port: u16) -> u8 {
        self.reads.push(port);
        0x7f
    }

    fn write_port(&mut self, port: u16, value: u8) {
        self.writes.push((port, value));
    }

    fn interrupt_acknowledge(&mut self) -> u8 {
        self.acknowledgements += 1;
        0xcf
    }

    fn interrupt_data(&mut self, address: u16, memory: &mut Memory) -> u8 {
        self.interrupt_address = Some(address);
        memory.read(address)
    }

    fn nmi_acknowledge(&mut self) {
        self.nmis += 1;
    }

    fn reti(&mut self) {
        self.reti_count += 1;
    }
}

#[test]
fn z80_executes_using_spectrum_memory_and_device_ports() {
    let program = [
        0x3e, 0x42, // LD A,42
        0x32, 0x00, 0x40, // LD (4000),A
        0x32, 0x00, 0x00, // LD (0000),A: ignored
        0x3e, 0x11, // LD A,11
        0x3a, 0x00, 0x40, // LD A,(4000)
        0x32, 0xff, 0xff, // LD (FFFF),A
        0xd3, 0xfe, // OUT (FE),A
        0xdb, 0xfe, // IN A,(FE)
        0x76, // HALT
    ];
    let mut rom = [0; ROM_SIZE];
    rom[..program.len()].copy_from_slice(&program);
    let mut memory = Memory::new(&rom).unwrap();
    let mut io = TestIo::default();
    let mut cpu = Z80::new();
    {
        let mut bus = SpectrumBus::new(&mut memory, &mut io);
        for expected in [7, 13, 13, 7, 13, 13, 11, 11, 4] {
            assert_eq!(cpu.step(&mut bus), expected);
        }
    }
    assert!(cpu.halted);
    assert_eq!(cpu.a, 0x7f);
    assert_eq!(memory.read(0), program[0]);
    assert_eq!(memory.read(0x4000), 0x42);
    assert_eq!(memory.read(0xffff), 0x42);
    assert_eq!(io.writes, [(0x42fe, 0x42)]);
    assert_eq!(io.reads, [0x42fe]);
}

#[test]
fn bus_forwards_interrupt_hooks_and_device_can_read_memory() {
    let mut memory = Memory::new(&[0x5a; ROM_SIZE]).unwrap();
    let mut io = TestIo::default();
    {
        let mut bus = SpectrumBus::new(&mut memory, &mut io);
        assert_eq!(bus.interrupt_acknowledge(), 0xcf);
        assert_eq!(bus.interrupt_data(0x1234), 0x5a);
        bus.nmi_acknowledge();
        bus.reti();
    }

    assert_eq!(io.acknowledgements, 1);
    assert_eq!(io.interrupt_address, Some(0x1234));
    assert_eq!(io.nmis, 1);
    assert_eq!(io.reti_count, 1);
}

#[test]
fn z80_scans_even_keyboard_ports_through_bus() {
    let program = [0x3e, 0xfd, 0xdb, 0xfe, 0x76]; // LD A,FD; IN A,(FE); HALT
    let mut rom = [0; ROM_SIZE];
    rom[..program.len()].copy_from_slice(&program);
    let mut memory = Memory::new(&rom).unwrap();
    let mut io = TestIo::default();
    let mut keyboard = KeyboardMatrix::new();
    keyboard.set_key(Key::A, true);
    let mut cpu = Z80::new();
    {
        let mut keyboard_io = KeyboardIo::new(&keyboard, &mut io);
        let mut bus = SpectrumBus::new(&mut memory, &mut keyboard_io);
        assert_eq!(cpu.step(&mut bus), 7);
        assert_eq!(cpu.step(&mut bus), 11);
        assert_eq!(cpu.a, 0x7e);
        assert_eq!(bus.read_port(0xfdff), 0x7f);
        assert_eq!(bus.read_port(0xfdfc), 0x7e);
    }
    assert_eq!(io.reads, [0xfdfe, 0xfdff, 0xfdfc]);
}
