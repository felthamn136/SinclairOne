use sinclair_core::{media::Media, services::input::InputEvent, Error, Machine};
use sinclair_cpu_z80::{Bus, Z80};
use sinclair_spectrum128::{
    bus::SpectrumBus,
    memory::{Memory, BANK_SIZE},
    Spectrum128,
};
use sinclair_spectrum48::{
    bus::SpectrumIo,
    keyboard::{Key, KeyboardIo, KeyboardMatrix},
    video::{FrameConfig, Ula, BORDER_LEFT, BORDER_TOP, PALETTE},
};

fn memory() -> Memory {
    Memory::new(&[0x11; BANK_SIZE], &[0x22; BANK_SIZE]).unwrap()
}

#[test]
fn validates_two_roms_and_initial_map() {
    assert!(matches!(
        Memory::new(&[], &[0; BANK_SIZE]),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        Memory::new(&[0; BANK_SIZE], &[0; BANK_SIZE + 1]),
        Err(Error::InvalidInput(_))
    ));
    let mut memory = memory();
    assert_eq!(memory.paging_register(), 0);
    assert_eq!(
        (
            memory.selected_rom(),
            memory.selected_bank(),
            memory.screen_bank()
        ),
        (0, 0, 5)
    );
    for address in [0, 0x3fff] {
        memory.write(address, 0xff);
        assert_eq!(memory.read(address), 0x11);
    }
    for bank in 0..8 {
        memory.write_port(0x7ffd, bank);
        for offset in 0..BANK_SIZE {
            assert_eq!(memory.read(0xc000 + offset as u16), 0);
        }
    }
    memory.write_port(0x7ffd, 0x10);
    for address in [0, 0x3fff] {
        memory.write(address, 0xff);
        assert_eq!(memory.read(address), 0x22);
    }
    memory.write_port(0x7ffd, 0);
    assert_eq!(memory.read(0), 0x11);
}

#[test]
fn all_banks_persist_and_fixed_banks_alias_the_paged_window() {
    let mut memory = memory();
    for bank in 0..8u8 {
        memory.write_port(0x7ffd, bank);
        memory.write(0xc000, bank + 0x10);
        memory.write(0xffff, bank + 0x20);
    }
    for bank in 0..8u8 {
        memory.write_port(0x7ffd, bank);
        assert_eq!(memory.read(0xc000), bank + 0x10);
        assert_eq!(memory.read(0xffff), bank + 0x20);
    }
    assert_eq!((memory.read(0x4000), memory.read(0x7fff)), (0x15, 0x25));
    assert_eq!((memory.read(0x8000), memory.read(0xbfff)), (0x12, 0x22));
    memory.write_port(0x7ffd, 5);
    memory.write(0x4000, 0xaa);
    assert_eq!(memory.read(0xc000), 0xaa);
    memory.write(0xffff, 0xbb);
    assert_eq!(memory.read(0x7fff), 0xbb);
    memory.write_port(0x7ffd, 2);
    memory.write(0x8000, 0xcc);
    assert_eq!(memory.read(0xc000), 0xcc);
    memory.write(0xffff, 0xdd);
    assert_eq!(memory.read(0xbfff), 0xdd);
}

#[test]
fn port_decoding_lock_and_reset_match_original_128k() {
    let mut memory = memory();
    for port in 0..=u16::MAX {
        memory.reset_paging();
        memory.write_port(port, 0x1f);
        assert_eq!(
            memory.paging_register(),
            if port & 0x8002 == 0 { 0x1f } else { 0 },
            "port {port:04x}"
        );
    }
    memory.write_port(0x7ffd, 0xff);
    assert_eq!(
        (
            memory.selected_bank(),
            memory.selected_rom(),
            memory.screen_bank()
        ),
        (7, 1, 7)
    );
    assert!(memory.paging_locked());
    memory.write(0xc123, 0x5a);
    for port in [0x7ffd, 0x0000, 0x7ffc] {
        memory.write_port(port, 0);
        assert_eq!(memory.paging_register(), 0xff);
    }
    memory.reset_paging();
    assert_eq!(memory.paging_register(), 0);
    assert!(!memory.paging_locked());
    memory.write_port(0x7ffd, 7);
    assert_eq!(memory.read(0xc123), 0x5a);
}

#[test]
fn display_bank_is_independent_of_cpu_paging_and_renders_both_screens() {
    let mut memory = memory();
    memory.write(0x4000, 0x80);
    memory.write(0x5800, 1);
    memory.write_port(0x7ffd, 7);
    memory.write(0xc000, 0x80);
    memory.write(0xd800, 2);
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 0,
        border_changes: &[],
    };
    let ula = Ula::new();
    let color_at = |row: &[u8]| row[BORDER_LEFT * 4..BORDER_LEFT * 4 + 4].to_vec();
    assert_eq!(
        color_at(&ula.render_scanline(&memory, &config, BORDER_TOP).unwrap()),
        PALETTE[0][1]
    );
    memory.write_port(0x7ffd, 8);
    assert_eq!((memory.selected_bank(), memory.screen_bank()), (0, 7));
    assert_eq!(memory.read(0x4000), 0x80);
    assert_eq!(memory.read(0x5800), 1);
    assert_eq!(
        color_at(&ula.render_scanline(&memory, &config, BORDER_TOP).unwrap()),
        PALETTE[0][2]
    );
    let frame = ula.render_frame(&memory, &config).unwrap();
    let offset = (BORDER_TOP * sinclair_spectrum48::video::FRAME_WIDTH + BORDER_LEFT) * 4;
    assert_eq!(&frame[offset..offset + 4], &PALETTE[0][2]);
    memory.write_port(0x7ffd, 7);
    assert_eq!(
        color_at(&ula.render_scanline(&memory, &config, BORDER_TOP).unwrap()),
        PALETTE[0][1]
    );
}

#[derive(Default)]
struct TestIo {
    writes: Vec<(u16, u8)>,
}

impl SpectrumIo<Memory> for TestIo {
    fn read_port(&mut self, _: u16) -> u8 {
        0xff
    }
    fn write_port(&mut self, port: u16, value: u8) {
        self.writes.push((port, value));
    }
    fn interrupt_acknowledge(&mut self) -> u8 {
        0xcf
    }
    fn interrupt_data(&mut self, address: u16, memory: &mut Memory) -> u8 {
        memory.read(address)
    }
    fn nmi_acknowledge(&mut self) {}
    fn reti(&mut self) {}
}

#[test]
fn z80_pages_ram_through_bus_and_io_and_keyboard_are_preserved() {
    let program = [
        0x01, 0xfd, 0x7f, // LD BC,7FFD
        0x3e, 0x07, // LD A,7
        0xed, 0x79, // OUT (C),A
        0x3e, 0x42, // LD A,42
        0x32, 0x00, 0xc0, // LD (C000),A
        0x76,
    ];
    let mut rom0 = [0; BANK_SIZE];
    rom0[..program.len()].copy_from_slice(&program);
    let mut memory = Memory::new(&rom0, &[0; BANK_SIZE]).unwrap();
    let mut io = TestIo::default();
    let mut keyboard = KeyboardMatrix::new();
    keyboard.set_key(Key::A, true);
    let mut cpu = Z80::new();
    {
        let mut keyboard_io = KeyboardIo::new(&keyboard, &mut io);
        let mut bus = SpectrumBus::new(&mut memory, &mut keyboard_io);
        for _ in 0..6 {
            cpu.step(&mut bus);
        }
        assert!(cpu.halted);
        assert_eq!(bus.read_port(0xfdfe), 0xfe);
        assert_eq!(bus.interrupt_acknowledge(), 0xcf);
        assert_eq!(bus.interrupt_data(0xc000), 0x42);
        bus.write_port(0x7ffd, 0);
        assert_eq!(bus.read_memory(0xc000), 0);
        bus.write_port(0x7ffd, 7);
        assert_eq!(bus.read_memory(0xc000), 0x42);
        bus.write_port(0xfffd, 0x10);
        assert_eq!(bus.read_memory(0), program[0]);
    }
    assert_eq!(
        io.writes,
        [(0x7ffd, 7), (0x7ffd, 0), (0x7ffd, 7), (0xfffd, 0x10)]
    );
}

#[test]
fn machine_reset_unlocks_paging_without_erasing_ram() {
    let mut machine = Spectrum128::new(&[0x11; BANK_SIZE], &[0x22; BANK_SIZE]).unwrap();
    assert_eq!(machine.info().id, "spectrum128");
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: true,
        })
        .unwrap();
    machine.memory_mut().write_port(0x7ffd, 0x3f);
    machine.memory_mut().write(0xc000, 0xa5);
    machine.reset().unwrap();
    assert_eq!(machine.memory().paging_register(), 0);
    assert_eq!(machine.memory().read(0), 0x11);
    assert_eq!(machine.keyboard().column_mask(0xfdfe), 0x1e);
    machine.memory_mut().write_port(0x7ffd, 7);
    assert_eq!(machine.memory().read(0xc000), 0xa5);
    assert!(matches!(machine.run_frame(), Err(Error::Unsupported(_))));
}

#[test]
fn machine_mounts_and_plays_tzx_without_replacing_on_failure() {
    let mut machine = Spectrum128::new(&[0; BANK_SIZE], &[0; BANK_SIZE]).unwrap();
    let bytes = b"ZXTape!\x1a\x01\x14\x12\x05\x00\x02\x00";
    machine
        .load_media(&Media {
            format: "tzx",
            bytes,
        })
        .unwrap();
    assert_eq!(machine.tzx_tape().unwrap().remaining(), 5);
    machine.tzx_tape_mut().unwrap().advance(2);
    assert!(machine
        .load_media(&Media {
            format: "tzx",
            bytes: &[0]
        })
        .is_err());
    assert_eq!(machine.tzx_tape().unwrap().remaining(), 3);
}
