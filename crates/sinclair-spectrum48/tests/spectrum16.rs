use sinclair_core::{services::input::InputEvent, state::StateManager, Machine};
use sinclair_spectrum48::{
    memory::ROM_SIZE,
    spectrum16::{Memory, Spectrum16},
    video::{BORDER_LEFT, BORDER_TOP, FRAME_WIDTH, PALETTE},
};

#[test]
fn memory_has_only_16k_ram_and_protected_rom() {
    let mut memory = Memory::new(&[0x5a; ROM_SIZE]).unwrap();
    assert!(Memory::new(&[0; 8192]).is_err());
    for address in [0x0000, 0x3fff] {
        memory.write(address, 0x12);
        assert_eq!(memory.read(address), 0x5a);
    }
    for address in [0x4000, 0x7fff] {
        memory.write(address, 0x42);
        assert_eq!(memory.read(address), 0x42);
    }
    for address in [0x8000, 0xffff] {
        memory.write(address, 0x42);
        assert_eq!(memory.read(address), 0xff);
    }
}

fn test_rom() -> Vec<u8> {
    let mut rom = vec![0; ROM_SIZE];
    // Scan the A row, store it in RAM, set a border/speaker and draw a pixel.
    let program = [
        0x3e, 0xfd, 0xdb, 0xfe, 0x32, 0x00, 0x60, 0x3e, 0x12, 0xd3, 0xfe, 0x3e, 0x80, 0x32, 0x00,
        0x40, 0x3e, 0x02, 0x32, 0x00, 0x58, 0x76,
    ];
    rom[..program.len()].copy_from_slice(&program);
    rom
}

#[test]
fn executes_cpu_keyboard_video_and_beeper_and_resets() {
    let mut machine = Spectrum16::new(&test_rom()).unwrap();
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: true,
        })
        .unwrap();
    let frame = machine.run_frame().unwrap();
    let offset = (BORDER_TOP * FRAME_WIDTH + BORDER_LEFT) * 4;
    assert_eq!(&frame.video.rgba[offset..offset + 4], &PALETTE[0][2]);
    assert_eq!(&frame.video.rgba[..4], &PALETTE[0][2]);
    assert!(frame.audio.samples.iter().any(|sample| *sample > 0.0));
    assert_eq!(machine.memory().read(0x6000) & 0x1f, 0x1e);
    machine.reset().unwrap();
    assert_eq!(machine.cpu().pc, 0);
    assert_eq!(machine.memory().read(0x6000) & 0x1f, 0x1e);
}

#[test]
fn save_load_and_rewind_replay_frames_and_reject_wrong_rom() {
    let mut machine = Spectrum16::new(&test_rom()).unwrap();
    let mut states = StateManager::new(2);
    states.run_frame(&mut machine).unwrap();
    states.quick_save(&machine).unwrap();
    let frame = states.run_frame(&mut machine).unwrap();
    let video = frame.video.rgba.to_vec();
    let audio = frame.audio.samples.to_vec();
    states.rewind(&mut machine).unwrap();
    let replay = states.run_frame(&mut machine).unwrap();
    assert_eq!(replay.video.rgba, video);
    assert_eq!(replay.audio.samples, audio);
    states.quick_load(&mut machine).unwrap();
    let replay = machine.run_frame().unwrap();
    assert_eq!(replay.video.rgba, video);
    assert_eq!(replay.audio.samples, audio);

    let before = machine.save_state().unwrap();
    let other = Spectrum16::new(&[0; ROM_SIZE])
        .unwrap()
        .save_state()
        .unwrap();
    assert!(machine.load_state(&other).is_err());
    assert_eq!(machine.save_state().unwrap(), before);
}

#[test]
#[ignore = "requires a user-supplied Spectrum ROM in SPECTRUM48_ROM"]
fn supplied_rom_boots_on_16k() {
    let path = std::env::var_os("SPECTRUM48_ROM").expect("set SPECTRUM48_ROM");
    let rom = std::fs::read(path).unwrap();
    let mut machine = Spectrum16::new(&rom).unwrap();
    for _ in 0..400 {
        machine.run_frame().unwrap();
    }
    let memory = machine.memory();
    let ramtop = u16::from_le_bytes([memory.read(0x5cb2), memory.read(0x5cb3)]);
    println!(
        "RAMTOP={ramtop:04x} PC={:04x} SP={:04x}",
        machine.cpu().pc,
        machine.cpu().sp
    );
    assert!((0x7000..=0x7fff).contains(&ramtop));
    assert!((0x4000..=0x7fff).contains(&machine.cpu().sp));
    assert!(memory.ram()[..6144].iter().any(|byte| *byte != 0));
    let before = memory.ram()[..6144].to_vec();
    let last_key = memory.read(0x5c08);
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: true,
        })
        .unwrap();
    for _ in 0..10 {
        machine.run_frame().unwrap();
    }
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: false,
        })
        .unwrap();
    for _ in 0..10 {
        machine.run_frame().unwrap();
    }
    assert_ne!(machine.memory().read(0x5c08), last_key);
    assert_ne!(&machine.memory().ram()[..6144], before);
}
