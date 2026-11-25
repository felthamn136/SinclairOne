use sinclair_core::{
    media::{Media, MediaCodec},
    Error, Machine,
};
use sinclair_cpu_z80::{Bus, InterruptMode, Z80};
use sinclair_spectrum48::{
    media::sna::{SnaCodec, SnaSnapshot, HEADER_SIZE, SNA_SIZE},
    memory::{Memory, RAM_SIZE, ROM_SIZE},
    Spectrum48,
};

fn known_sna() -> Vec<u8> {
    let mut bytes = vec![0; SNA_SIZE];
    bytes[..HEADER_SIZE].copy_from_slice(&[
        0x11, 0x21, 0x22, 0x31, 0x32, 0x41, 0x42, 0x51, 0x52, 0x61, 0x62, 0x71, 0x72, 0x81, 0x82,
        0x91, 0x92, 0xa1, 0xa2, 0x04, 0xb1, 0xc1, 0xc2, 0x00, 0x80, 0x02, 0x05,
    ]);
    for (index, byte) in bytes[HEADER_SIZE..].iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(37).wrapping_add(13);
    }
    bytes[HEADER_SIZE + 0x4000..HEADER_SIZE + 0x4002].copy_from_slice(&[0x34, 0x12]);
    bytes
}

#[test]
fn parses_and_serializes_a_known_layout() {
    let bytes = known_sna();
    let snapshot = SnaCodec.decode(&bytes).unwrap();
    let cpu = snapshot.cpu();
    assert_eq!((cpu.i, cpu.r), (0x11, 0xb1));
    assert_eq!(
        (cpu.hl_alt(), cpu.de_alt(), cpu.bc_alt(), cpu.af_alt()),
        (0x2221, 0x3231, 0x4241, 0x5251)
    );
    assert_eq!(
        (cpu.hl(), cpu.de(), cpu.bc(), cpu.af()),
        (0x6261, 0x7271, 0x8281, 0xc2c1)
    );
    assert_eq!((cpu.iy, cpu.ix), (0x9291, 0xa2a1));
    assert_eq!((cpu.pc, cpu.sp), (0x1234, 0x8002));
    assert!(cpu.iff1 && cpu.iff2);
    assert_eq!(cpu.im, InterruptMode::Mode2);
    assert_eq!(snapshot.border_colour(), 5);
    assert_eq!(snapshot.ram(), &bytes[HEADER_SIZE..]);
    assert_eq!(snapshot.to_bytes().unwrap(), bytes);
}

#[test]
fn captures_without_changing_ram_and_pushes_pc_little_endian() {
    let mut memory = Memory::new(&[0xa5; ROM_SIZE]).unwrap();
    let ram: Vec<u8> = (0..RAM_SIZE).map(|i| (i >> 8) as u8).collect();
    memory.load_ram(&ram).unwrap();
    let mut cpu = Z80::new();
    cpu.sp = 0x4002;
    cpu.pc = 0xabcd;
    cpu.iff1 = true;
    cpu.iff2 = true;
    cpu.im = InterruptMode::Mode1;
    let bytes = SnaSnapshot::capture(&cpu, &memory, 7)
        .unwrap()
        .to_bytes()
        .unwrap();
    assert_eq!(&bytes[23..27], &[0x00, 0x40, 1, 7]);
    assert_eq!(&bytes[HEADER_SIZE..HEADER_SIZE + 2], &[0xcd, 0xab]);
    assert_eq!(&bytes[HEADER_SIZE + 2..], &ram[2..]);
    assert_eq!(memory.ram(), ram);
    assert_eq!(memory.read(0x3fff), 0xa5);

    cpu.sp = 0;
    let bytes = SnaSnapshot::capture(&cpu, &memory, 0)
        .unwrap()
        .to_bytes()
        .unwrap();
    assert_eq!(&bytes[23..25], &[0xfe, 0xff]);
    assert_eq!(&bytes[SNA_SIZE - 2..], &[0xcd, 0xab]);
    assert_eq!(SnaSnapshot::parse(&bytes).unwrap().cpu().sp, 0);
}

#[test]
fn rejects_malformed_snapshots_and_unrepresentable_stack() {
    let bytes = known_sna();
    for length in [0, HEADER_SIZE - 1, SNA_SIZE - 1] {
        assert!(SnaSnapshot::parse(&bytes[..length]).is_err());
    }
    let mut invalid = bytes.clone();
    invalid.push(0);
    assert!(SnaSnapshot::parse(&invalid).is_err());
    for sp in [0, 0x3fff, 0xffff] {
        let mut invalid = bytes.clone();
        invalid[23..25].copy_from_slice(&u16::to_le_bytes(sp));
        assert!(SnaSnapshot::parse(&invalid).is_err());
    }
    for (offset, value) in [(25, 3), (26, 8)] {
        let mut invalid = bytes.clone();
        invalid[offset] = value;
        assert!(SnaSnapshot::parse(&invalid).is_err());
    }
    let memory = Memory::new(&[0; ROM_SIZE]).unwrap();
    for sp in [1, 2, 0x4000, 0x4001] {
        let mut cpu = Z80::new();
        cpu.sp = sp;
        assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
    }
    assert!(SnaSnapshot::capture(&Z80::new(), &memory, 8).is_err());
}

#[test]
fn interrupt_flags_follow_retn_and_other_header_bits_are_ignored() {
    for (header, expected) in [(0, false), (1, false), (4, true), (0xff, true)] {
        let mut bytes = known_sna();
        bytes[19] = header;
        let snapshot = SnaSnapshot::parse(&bytes).unwrap();
        assert_eq!(
            (snapshot.cpu().iff1, snapshot.cpu().iff2),
            (expected, expected)
        );
        assert_eq!(
            snapshot.to_bytes().unwrap()[19],
            if expected { 4 } else { 0 }
        );
    }
    for mode in 0..=2 {
        let mut bytes = known_sna();
        bytes[25] = mode;
        assert_eq!(
            SnaSnapshot::parse(&bytes).unwrap().to_bytes().unwrap()[25],
            mode
        );
    }
}

struct EiBus;
impl Bus for EiBus {
    fn read_memory(&mut self, _: u16) -> u8 {
        0xfb
    }
    fn write_memory(&mut self, _: u16, _: u8) {}
    fn read_port(&mut self, _: u16) -> u8 {
        0
    }
    fn write_port(&mut self, _: u16, _: u8) {}
}

#[test]
fn rejects_cpu_state_the_format_cannot_restore() {
    let memory = Memory::new(&[0; ROM_SIZE]).unwrap();
    let mut cpu = Z80::new();
    cpu.iff1 = true;
    assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
    cpu.iff2 = true;
    cpu.halted = true;
    assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
    cpu.halted = false;
    cpu.set_irq(true);
    assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
    cpu.set_irq(false);
    cpu.request_nmi();
    assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
    let mut cpu = Z80::new();
    cpu.step(&mut EiBus);
    assert!(SnaSnapshot::capture(&cpu, &memory, 0).is_err());
}

#[test]
fn machine_load_and_save_are_transactional_and_preserve_rom() {
    let bytes = known_sna();
    let mut machine = Spectrum48::new(&[0xa5; ROM_SIZE]).unwrap();
    machine
        .load_media(&Media {
            format: "sna",
            bytes: &bytes,
        })
        .unwrap();
    assert_eq!(machine.cpu().pc, 0x1234);
    assert_eq!(machine.cpu().sp, 0x8002);
    assert_eq!(machine.border_colour(), 5);
    assert_eq!(machine.memory().ram(), &bytes[HEADER_SIZE..]);
    assert_eq!(machine.memory().read(0), 0xa5);
    assert_eq!(machine.memory().read(0x3fff), 0xa5);
    assert_eq!(machine.save_sna().unwrap(), bytes);
    for invalid in [&bytes[..100], &[][..]] {
        assert!(matches!(
            machine.load_sna(invalid),
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(machine.save_sna().unwrap(), bytes);
    }
    let mut invalid = bytes.clone();
    invalid[25] = 3;
    assert!(machine
        .load_media(&Media {
            format: "sna",
            bytes: &invalid
        })
        .is_err());
    assert_eq!(machine.save_sna().unwrap(), bytes);
}
