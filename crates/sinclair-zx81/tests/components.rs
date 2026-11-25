use sinclair_core::{
    media::{Media, MediaCodec},
    services::input::InputEvent,
    state::StateManager,
    Error, Machine,
};
use sinclair_zx81::{
    memory::{Memory, RamSize, ROM_SIZE},
    program::{Program, ProgramCodec, LOAD_ADDRESS},
    tape::{WavTape, CPU_HZ},
    video::{render_display, HEIGHT, WIDTH},
    Zx81,
};

fn program(length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    let end = LOAD_ADDRESS + length as u16;
    bytes[11..13].copy_from_slice(&end.to_le_bytes());
    bytes[length - 1] = 0xa5;
    bytes
}

fn wav(channels: u16, bits: u16, samples: &[u8]) -> Vec<u8> {
    let rate = 32_500u32;
    let align = channels * bits / 8;
    let data_len = samples.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len + data_len % 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    bytes.extend_from_slice(&align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.extend_from_slice(samples);
    if data_len % 2 != 0 {
        bytes.push(0);
    }
    bytes
}

#[test]
fn memory_decodes_rom_and_both_ram_configurations() {
    let mut rom = vec![0; ROM_SIZE];
    rom[0] = 0x12;
    rom[ROM_SIZE - 1] = 0x34;
    assert!(Memory::new(&rom[..ROM_SIZE - 1], RamSize::OneK).is_err());
    let mut small = Memory::new(&rom, RamSize::OneK).unwrap();
    for address in [0, 0x2000, 0x8000, 0xa000] {
        small.write(address, 0xff);
        assert_eq!(small.read(address), 0x12);
        assert_eq!(small.read(address + 0x1fff), 0x34);
    }
    small.write(0x4000, 0x56);
    for address in [0x4000, 0x4400, 0x7c00, 0xc000, 0xc400, 0xfc00] {
        assert_eq!(small.read(address), 0x56);
    }
    let mut expanded = Memory::new(&rom, RamSize::SixteenK).unwrap();
    expanded.write(0x4000, 0x11);
    expanded.write(0x4400, 0x22);
    expanded.write(0x7fff, 0x33);
    assert_eq!(expanded.read(0xc000), 0x11);
    assert_eq!(expanded.read(0xc400), 0x22);
    assert_eq!(expanded.read(0xffff), 0x33);
}

#[test]
fn program_images_fit_the_installed_ram_and_validate_before_copying() {
    let small_image = program(1024 - 9);
    let image = ProgramCodec.decode(&small_image).unwrap();
    let mut small = Memory::new(&[0; ROM_SIZE], RamSize::OneK).unwrap();
    image.copy_to_memory(&mut small).unwrap();
    assert_eq!(&small.ram()[9..], &small_image);

    let big_image = program(2048);
    let big = Program::parse(&big_image).unwrap();
    let before = small.ram().to_vec();
    assert!(big.copy_to_memory(&mut small).is_err());
    assert_eq!(small.ram(), before);
    let mut expanded = Memory::new(&[0; ROM_SIZE], RamSize::SixteenK).unwrap();
    big.copy_to_memory(&mut expanded).unwrap();
    assert_eq!(&expanded.ram()[9..9 + big_image.len()], &big_image);

    assert!(Program::parse(&small_image[..115]).is_err());
    let mut bad = small_image;
    bad[11] ^= 1;
    assert!(Program::parse(&bad).is_err());
}

#[test]
fn collapsed_display_renders_rom_glyphs_and_inverse_characters() {
    let mut rom = [0; ROM_SIZE];
    rom[0x1e08] = 0x80;
    let mut memory = Memory::new(&rom, RamSize::OneK).unwrap();
    memory.write(0x400c, 0x80);
    memory.write(0x400d, 0x40);
    memory.write(0x4080, 0x76);
    memory.write(0x4081, 0x01);
    memory.write(0x4082, 0x81);
    for address in 0x4083..0x409b {
        memory.write(address, 0x76);
    }

    let pixels = render_display(&memory).unwrap();
    assert_eq!(pixels.len(), WIDTH * HEIGHT * 4);
    assert_eq!(&pixels[0..4], &[0, 0, 0, 255]);
    assert_eq!(&pixels[4..8], &[255, 255, 255, 255]);
    assert_eq!(&pixels[32..36], &[255, 255, 255, 255]);
    assert_eq!(&pixels[36..40], &[0, 0, 0, 255]);
    memory.write(0x4083, 0x40);
    assert!(render_display(&memory).is_err());
}

#[test]
fn machine_input_program_and_quick_state_use_shared_interfaces() {
    let mut machine = Zx81::new(&[0x5a; ROM_SIZE], RamSize::OneK).unwrap();
    let bytes = program(1024 - 9);
    machine
        .load_media(&Media {
            format: "p",
            bytes: &bytes,
        })
        .unwrap();
    machine
        .input(InputEvent {
            control: ".".into(),
            pressed: true,
        })
        .unwrap();
    let mut manager = StateManager::new(1);
    manager.quick_save(&machine).unwrap();
    machine.memory_mut().write(0x43ff, 0x11);
    machine
        .input(InputEvent {
            control: ".".into(),
            pressed: false,
        })
        .unwrap();
    manager.quick_load(&mut machine).unwrap();
    assert_eq!(machine.memory().read(0x43ff), 0xa5);
    assert_eq!(machine.keyboard().column_mask(0x7ffe), 0x1d);
    assert!(matches!(machine.run_frame(), Err(Error::Unsupported(_))));

    let state = machine.save_state().unwrap();
    let mut incompatible = Zx81::new(&[0x5a; ROM_SIZE], RamSize::SixteenK).unwrap();
    let before = incompatible.save_state().unwrap();
    assert!(incompatible.load_state(&state).is_err());
    assert_eq!(incompatible.save_state().unwrap(), before);
}

#[test]
fn wav_input_advances_fractionally_and_survives_quick_load() {
    let bytes = wav(1, 8, &[0, 255, 128]);
    let mut machine = Zx81::new(&[0; ROM_SIZE], RamSize::OneK).unwrap();
    machine
        .load_media(&Media {
            format: "wav",
            bytes: &bytes,
        })
        .unwrap();
    let tape = machine.tape_mut().unwrap();
    assert!(!tape.level());
    assert_eq!(tape.advance(99), 99);
    assert!(!tape.level());
    assert_eq!(tape.advance(18), 18);
    assert!(tape.level());
    let mut manager = StateManager::new(1);
    manager.quick_save(&machine).unwrap();
    assert_eq!(machine.tape_mut().unwrap().advance(183), 183);
    assert!(machine.tape().unwrap().finished());
    manager.quick_load(&mut machine).unwrap();
    let tape = machine.tape_mut().unwrap();
    assert!(tape.level());
    assert_eq!(tape.advance(82), 82);
    assert!(tape.level());
    assert_eq!(tape.advance(1), 1);
    assert!(!tape.level());
    assert_eq!(tape.advance(1000), 100);
    assert!(tape.finished());
    assert_eq!(tape.advance(1), 0);

    let stereo = wav(2, 16, &[0xff, 0x7f, 0xff, 0x7f, 0, 0x80, 0, 0x80]);
    let mut tape = WavTape::parse(&stereo).unwrap();
    assert!(tape.level());
    tape.advance(100);
    assert!(!tape.level());
    let mut fractional = bytes.clone();
    fractional[24..28].copy_from_slice(&44_100u32.to_le_bytes());
    fractional[28..32].copy_from_slice(&44_100u32.to_le_bytes());
    let mut tape = WavTape::parse(&fractional).unwrap();
    tape.advance(73);
    assert!(!tape.level());
    tape.advance(1);
    assert!(tape.level());
    tape.advance(73);
    assert!(tape.level());
    tape.advance(1);
    assert!(!tape.level());
    let before = machine.save_state().unwrap();
    assert!(machine
        .load_media(&Media {
            format: "wav",
            bytes: &bytes[..bytes.len() - 1],
        })
        .is_err());
    assert_eq!(machine.save_state().unwrap(), before);
}

#[test]
fn cpu_bus_distinguishes_display_fetches_from_data_reads() {
    let mut rom = [0; ROM_SIZE];
    rom[..11].copy_from_slice(&[
        0x3e, 0x24, 0x32, 0x00, 0x40, 0x3a, 0x00, 0xc0, 0xc3, 0x00, 0xc0,
    ]);
    let mut machine = Zx81::new(&rom, RamSize::OneK).unwrap();
    for _ in 0..5 {
        machine.step_instruction();
    }
    assert_eq!(machine.cpu().a, 0x24);
    assert_eq!(machine.cpu().pc, 0xc001);
    assert_eq!(machine.memory().read(0xc000), 0x24);
}

#[test]
fn cpu_reads_keyboard_and_wav_input_through_port_fe() {
    let mut rom = [0; ROM_SIZE];
    rom[..7].copy_from_slice(&[0x3e, 0x7f, 0xdb, 0xfe, 0x32, 0x00, 0x40]);
    let mut machine = Zx81::new(&rom, RamSize::OneK).unwrap();
    machine
        .input(InputEvent {
            control: ".".into(),
            pressed: true,
        })
        .unwrap();
    let audio = wav(1, 8, &[255, 0]);
    machine
        .load_media(&Media {
            format: "wav",
            bytes: &audio,
        })
        .unwrap();
    for _ in 0..3 {
        machine.step_instruction();
    }
    assert_eq!(machine.memory().read(0x4000), 0xfd);
    let mut manager = StateManager::new(1);
    manager.quick_save(&machine).unwrap();
    for _ in 0..30 {
        machine.step_instruction();
    }
    let after = machine.save_state().unwrap();
    manager.quick_load(&mut machine).unwrap();
    for _ in 0..30 {
        machine.step_instruction();
    }
    assert_eq!(machine.save_state().unwrap(), after);
}

#[test]
fn periodic_nmi_can_be_enabled_and_disabled_via_ports() {
    let mut rom = [0; ROM_SIZE];
    rom[..15].copy_from_slice(&[
        0x31, 0xf0, 0x43, 0xaf, 0xd3, 0xfe, 0x06, 0x40, 0x10, 0xfe, 0xd3, 0xfd, 0xc3, 0x0c, 0x00,
    ]);
    rom[0x66..0x6f].copy_from_slice(&[0x3a, 0x00, 0x40, 0x3c, 0x32, 0x00, 0x40, 0xed, 0x45]);
    let mut machine = Zx81::new(&rom, RamSize::OneK).unwrap();
    for _ in 0..300 {
        machine.step_instruction();
    }
    let count = machine.memory().read(0x4000);
    assert!(count > 0);
    assert_eq!(machine.cpu().pc, 0x000c);
    for _ in 0..100 {
        machine.step_instruction();
    }
    assert_eq!(machine.memory().read(0x4000), count);
}

#[test]
fn display_interrupt_follows_refresh_address_bit_six() {
    let mut rom = [0; ROM_SIZE];
    rom[..11].copy_from_slice(&[
        0x31, 0xf0, 0x43, 0xed, 0x56, 0x3e, 0x7d, 0xed, 0x4f, 0xfb, 0x76,
    ]);
    rom[0x38..0x3e].copy_from_slice(&[0x3e, 0x66, 0x32, 0x00, 0x40, 0x76]);
    let mut machine = Zx81::new(&rom, RamSize::OneK).unwrap();
    for _ in 0..20 {
        machine.step_instruction();
    }
    assert_eq!(machine.memory().read(0x4000), 0x66);
}

#[test]
fn program_tape_emits_zx81_bursts_and_restores_mid_pulse() {
    let bytes = program(116);
    let mut machine = Zx81::new(&[0; ROM_SIZE], RamSize::OneK).unwrap();
    machine
        .load_media(&Media {
            format: "p-tape",
            bytes: &bytes,
        })
        .unwrap();
    assert_eq!(machine.memory().read(0x4014), 0);
    let tape = machine.tape_mut().unwrap();
    assert!(!tape.level());
    assert_eq!(tape.advance(CPU_HZ * 5 - 1), CPU_HZ * 5 - 1);
    assert!(!tape.level());
    tape.advance(1);
    for wave in 0..9 {
        assert!(tape.level());
        tape.advance(503);
        assert!(!tape.level());
        tape.advance(if wave == 8 { 4_861 } else { 472 });
    }
    for wave in 0..4 {
        assert!(tape.level());
        tape.advance(503);
        assert!(!tape.level());
        if wave < 3 {
            tape.advance(472);
        }
    }
    tape.advance(127);
    let mut manager = StateManager::new(1);
    manager.quick_save(&machine).unwrap();
    machine.tape_mut().unwrap().advance(4_733);
    assert!(!machine.tape().unwrap().level());
    machine.tape_mut().unwrap().advance(1);
    assert!(machine.tape().unwrap().level());
    manager.quick_load(&mut machine).unwrap();
    let tape = machine.tape_mut().unwrap();
    tape.advance(4_733);
    assert!(!tape.level());
    tape.advance(1);
    assert!(tape.level());
    let consumed = tape.advance(u64::MAX);
    assert!(consumed < u64::MAX);
    assert!(tape.finished());
    assert_eq!(tape.advance(1), 0);
}

#[test]
fn cpu_can_sample_program_tape_signal() {
    let mut rom = [0; ROM_SIZE];
    rom[..7].copy_from_slice(&[0x3e, 0x7f, 0xdb, 0xfe, 0x32, 0x00, 0x40]);
    let mut machine = Zx81::new(&rom, RamSize::OneK).unwrap();
    let bytes = program(116);
    machine
        .load_media(&Media {
            format: "p-tape",
            bytes: &bytes,
        })
        .unwrap();
    machine.tape_mut().unwrap().advance(CPU_HZ * 5);
    for _ in 0..3 {
        machine.step_instruction();
    }
    assert_eq!(machine.memory().read(0x4000) & 0x80, 0x80);
}
