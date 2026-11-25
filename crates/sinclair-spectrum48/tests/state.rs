use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateReader, StateWriter},
    Error, Machine,
};
use sinclair_spectrum48::{
    media::{
        sna::{HEADER_SIZE, SNA_SIZE},
        tap::{TapDeck, TapImage},
        tzx::{PlaybackStatus, TzxDeck, TzxImage},
    },
    memory::{Memory, RAM_SIZE, ROM_SIZE},
    Spectrum48,
};

fn sna(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0; SNA_SIZE];
    bytes[0] = seed;
    bytes[1..19].fill(seed.wrapping_add(1));
    bytes[19] = 4;
    bytes[20] = seed.wrapping_add(2);
    bytes[21..23].copy_from_slice(&[0x31, 0x42]);
    bytes[23..25].copy_from_slice(&0x8000u16.to_le_bytes());
    bytes[25] = 2;
    bytes[26] = 5;
    for (i, byte) in bytes[HEADER_SIZE..].iter_mut().enumerate() {
        *byte = (i as u8).wrapping_add(seed);
    }
    bytes[HEADER_SIZE + 0x4000..HEADER_SIZE + 0x4002].copy_from_slice(&[0x34, 0x12]);
    bytes
}

fn tzx() -> Vec<u8> {
    let mut bytes = b"ZXTape!\x1a\x01\x14".to_vec();
    bytes.push(0x11);
    for value in [10u16, 20, 30, 40, 50, 3] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&[3, 3, 0, 1, 0, 0, 0xa0]);
    bytes.extend_from_slice(&[0x12, 4, 0, 2, 0, 0x13, 2, 5, 0, 6, 0]);
    bytes.extend_from_slice(&[0x20, 0, 0, 0x14, 7, 0, 9, 0, 1, 0, 0, 1, 0, 0, 0x80]);
    bytes
}

fn mount(machine: &mut Spectrum48, format: &str, bytes: &[u8]) {
    machine.load_media(&Media { format, bytes }).unwrap();
}

#[test]
fn saves_and_restores_cpu_ram_border_keyboard_and_tape() {
    let mut machine = Spectrum48::new(&[0xa5; ROM_SIZE]).unwrap();
    mount(&mut machine, "sna", &sna(17));
    mount(&mut machine, "tzx", &tzx());
    machine.tzx_tape_mut().unwrap().advance(17);
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: true,
        })
        .unwrap();
    let saved = machine.save_state().unwrap();
    let cpu = machine.cpu().state_bytes();
    let ram = machine.memory().ram().to_vec();
    let tape = (
        machine.tzx_tape().unwrap().level(),
        machine.tzx_tape().unwrap().remaining(),
    );
    mount(&mut machine, "sna", &sna(44));
    mount(&mut machine, "tap", include_bytes!("fixtures/basic.tap"));
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: false,
        })
        .unwrap();
    machine.load_state(&saved).unwrap();
    assert_eq!(machine.cpu().state_bytes(), cpu);
    assert_eq!(machine.memory().ram(), ram);
    assert_eq!(machine.memory().read(0), 0xa5);
    assert_eq!(machine.border_colour(), 5);
    assert_eq!(machine.keyboard().column_mask(0xfdfe), 0x1e);
    assert!(machine.tape().is_none());
    assert_eq!(
        (
            machine.tzx_tape().unwrap().level(),
            machine.tzx_tape().unwrap().remaining()
        ),
        tape
    );
    assert_eq!(machine.save_state().unwrap(), saved);
}

#[test]
fn restores_mounted_tap_after_another_tape_is_loaded() {
    let mut machine = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    let tap = include_bytes!("fixtures/basic.tap");
    mount(&mut machine, "tap", tap);
    let saved = machine.save_state().unwrap();
    mount(&mut machine, "tzx", &tzx());
    machine.load_state(&saved).unwrap();
    assert!(machine.tzx_tape().is_none());
    assert_eq!(machine.tape().unwrap().position(), 0);
    assert_eq!(machine.tape().unwrap().image().blocks().len(), 2);
    assert_eq!(machine.save_state().unwrap(), saved);
}

#[test]
fn restores_advanced_tap_cursor() {
    let image = TapImage::parse(include_bytes!("fixtures/basic.tap")).unwrap();
    let mut deck = TapDeck::new(image);
    deck.load_next_basic(&mut Memory::new(&[0; ROM_SIZE]).unwrap(), 0x8000)
        .unwrap();
    let mut writer = StateWriter::new();
    deck.write_state(&mut writer).unwrap();
    let bytes = writer.finish();
    let mut reader = StateReader::new(&bytes);
    let restored = TapDeck::read_state(&mut reader).unwrap();
    reader.finish().unwrap();
    assert_eq!(restored.position(), 2);
    assert_eq!(restored.image(), deck.image());
}

#[test]
fn rejects_bad_states_transactionally_and_requires_matching_rom() {
    let mut machine = Spectrum48::new(&[0xa5; ROM_SIZE]).unwrap();
    mount(&mut machine, "sna", &sna(17));
    let original = machine.save_state().unwrap();
    let mut invalid = original.clone();
    invalid.payload[100] ^= 1;
    assert!(matches!(
        machine.load_state(&invalid),
        Err(Error::IncompatibleState(_))
    ));
    assert_eq!(machine.save_state().unwrap(), original);
    let mut payload = original.checked_payload("spectrum48", 1).unwrap().to_vec();
    let border = 4 + ROM_SIZE + 4 + 38 + 4 + RAM_SIZE;
    payload[border] = 8;
    let invalid = State::sealed("spectrum48", 1, payload);
    assert!(machine.load_state(&invalid).is_err());
    assert_eq!(machine.save_state().unwrap(), original);
    let mut invalid = original.clone();
    invalid.version += 1;
    assert!(machine.load_state(&invalid).is_err());
    assert_eq!(machine.save_state().unwrap(), original);
    let mut other = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    let before = other.save_state().unwrap();
    assert!(other.load_state(&original).is_err());
    assert_eq!(other.save_state().unwrap(), before);
}

fn clone_deck(deck: &TzxDeck) -> TzxDeck {
    let mut writer = StateWriter::new();
    deck.write_state(&mut writer).unwrap();
    let bytes = writer.finish();
    let mut reader = StateReader::new(&bytes);
    let restored = TzxDeck::read_state(&mut reader).unwrap();
    reader.finish().unwrap();
    restored
}

#[test]
fn restores_tape_at_each_pulse_pause_stop_and_end() {
    let mut deck = TzxDeck::new(TzxImage::parse(&tzx()).unwrap());
    let mut seen_stop = false;
    for _ in 0..512 {
        let mut restored = clone_deck(&deck);
        assert_eq!(restored.status(), deck.status());
        assert_eq!(restored.level(), deck.level());
        assert_eq!(restored.remaining(), deck.remaining());
        assert_eq!(restored.block_index(), deck.block_index());
        match deck.status() {
            PlaybackStatus::Playing => {
                let step = (deck.remaining() / 2).max(1);
                assert_eq!(restored.advance(step), deck.advance(step));
                assert_eq!(
                    (restored.level(), restored.remaining(), restored.status()),
                    (deck.level(), deck.remaining(), deck.status())
                );
            }
            PlaybackStatus::Stopped => {
                seen_stop = true;
                assert!(deck.resume());
                assert!(restored.resume());
            }
            PlaybackStatus::Finished => {
                assert!(seen_stop);
                return;
            }
        }
    }
    panic!("tape did not finish");
}

#[test]
fn rejects_corrupt_tape_playback_state() {
    let deck = TzxDeck::new(TzxImage::parse(&tzx()).unwrap());
    let mut writer = StateWriter::new();
    deck.write_state(&mut writer).unwrap();
    let mut bytes = writer.finish();
    let phase = 4 + tzx().len() + 8;
    bytes[phase] = 255;
    assert!(TzxDeck::read_state(&mut StateReader::new(&bytes)).is_err());
}
