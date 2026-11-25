use sinclair_core::{
    media::{Media, MediaCodec},
    Error, Machine,
};
use sinclair_spectrum48::{
    media::tzx::{PlaybackStatus, TzxBlock, TzxCodec, TzxDeck, TzxImage, T_STATES_PER_MS},
    memory::ROM_SIZE,
    Spectrum48,
};

fn header() -> Vec<u8> {
    b"ZXTape!\x1a\x01\x14".to_vec()
}

fn standard(flag: u8, pause_ms: u16) -> Vec<u8> {
    let mut bytes = header();
    bytes.push(0x10);
    bytes.extend_from_slice(&pause_ms.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.push(flag);
    bytes
}

fn turbo() -> Vec<u8> {
    let mut bytes = header();
    bytes.push(0x11);
    for word in [10u16, 20, 30, 40, 50, 3] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.push(3);
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0xa0]); // 101, MSB first
    bytes
}

fn pulse(deck: &mut TzxDeck, level: bool, duration: u64) {
    assert_eq!(deck.status(), PlaybackStatus::Playing);
    assert_eq!(deck.level(), level);
    assert_eq!(deck.remaining(), duration);
    assert_eq!(deck.advance(duration), duration);
}

#[test]
fn standard_blocks_have_documented_header_and_data_pilot_lengths() {
    for (flag, count) in [(0u8, 8063), (0x80, 3223)] {
        let bytes = standard(flag, 0);
        let image = TzxCodec.decode(&bytes).unwrap();
        assert_eq!(image.version(), (1, 20));
        let TzxBlock::Standard(block) = &image.blocks()[0] else {
            panic!("standard block")
        };
        assert_eq!(block.pilot_count, count);
        assert_eq!(block.data, [flag]);
        let mut deck = TzxDeck::new(image);
        for i in 0..count {
            pulse(&mut deck, i % 2 != 0, 2168);
        }
        pulse(&mut deck, true, 667);
        pulse(&mut deck, false, 735);
        for bit in 0..8 {
            let duration = if flag & (0x80 >> bit) != 0 { 1710 } else { 855 };
            pulse(&mut deck, true, duration);
            pulse(&mut deck, false, duration);
        }
        assert_eq!(deck.status(), PlaybackStatus::Finished);
        assert_eq!(deck.advance(100), 0);
    }
}

#[test]
fn turbo_uses_custom_timings_partial_byte_and_pause_levels() {
    let image = TzxImage::parse(&turbo()).unwrap();
    let mut deck = TzxDeck::new(image);
    assert_eq!(deck.advance(7), 7);
    assert!(!deck.level());
    assert_eq!(deck.remaining(), 3);
    assert_eq!(deck.advance(3), 3);
    pulse(&mut deck, true, 10);
    pulse(&mut deck, false, 10);
    pulse(&mut deck, true, 20);
    pulse(&mut deck, false, 30);
    for (level, duration) in [
        (true, 50),
        (false, 50),
        (true, 40),
        (false, 40),
        (true, 50),
        (false, 50),
    ] {
        pulse(&mut deck, level, duration);
    }
    pulse(&mut deck, true, T_STATES_PER_MS);
    pulse(&mut deck, false, T_STATES_PER_MS);
    assert_eq!(deck.status(), PlaybackStatus::Finished);
    assert!(!deck.level());
    deck.rewind();
    assert_eq!(deck.remaining(), 10);
    assert!(!deck.level());
}

#[test]
fn turbo_lengths_are_24_bit_and_partial_bits_apply_only_to_last_byte() {
    let mut bytes = turbo();
    bytes.truncate(bytes.len() - 4);
    bytes.extend_from_slice(&[2, 0, 0, 0x00, 0x80]);
    bytes[23] = 1;
    let mut deck = TzxDeck::new(TzxImage::parse(&bytes).unwrap());
    assert_eq!(deck.advance(3 * 10 + 20 + 30), 80);
    for _ in 0..8 {
        assert_eq!(deck.remaining(), 40);
        assert_eq!(deck.advance(80), 80);
    }
    assert_eq!(deck.remaining(), 50);
    assert_eq!(deck.advance(100), 100);
    assert_eq!(deck.remaining(), T_STATES_PER_MS);

    let mut large = turbo();
    large.truncate(large.len() - 4);
    large.extend_from_slice(&[0, 0, 1]);
    large.extend(std::iter::repeat(0).take(65_536));
    let image = TzxImage::parse(&large).unwrap();
    let TzxBlock::Turbo(block) = &image.blocks()[0] else {
        panic!("turbo block")
    };
    assert_eq!(block.data.len(), 65_536);
    large.pop();
    assert!(TzxImage::parse(&large).is_err());
}

#[test]
fn pure_tone_sequence_data_stop_and_signal_level_compose_without_gaps() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x12, 4, 0, 2, 0]); // two 4T pulses
    bytes.extend_from_slice(&[0x13, 2, 5, 0, 6, 0]);
    bytes.extend_from_slice(&[0x2b, 1, 0, 0, 0, 1]); // set high
    bytes.extend_from_slice(&[0x14, 7, 0, 9, 0, 2, 0, 0, 1, 0, 0, 0x80]);
    bytes.extend_from_slice(&[0x20, 0, 0]); // stop
    bytes.extend_from_slice(&[0x30, 1, b'X']); // metadata
    bytes.extend_from_slice(&[0x12, 3, 0, 1, 0]);
    let mut deck = TzxDeck::new(TzxImage::parse(&bytes).unwrap());
    for (level, duration) in [
        (false, 4),
        (true, 4),
        (false, 5),
        (true, 6),
        (true, 9),
        (false, 9),
        (true, 7),
        (false, 7),
    ] {
        pulse(&mut deck, level, duration);
    }
    assert_eq!(deck.status(), PlaybackStatus::Stopped);
    assert!(deck.level());
    assert_eq!(deck.advance(100), 0);
    assert!(deck.resume());
    pulse(&mut deck, true, 3);
    assert_eq!(deck.status(), PlaybackStatus::Finished);
    assert!(!deck.resume());
}

#[test]
fn exact_time_consumption_stops_at_boundaries() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x12, 2, 0, 2, 0, 0x20, 0, 0, 0x12, 5, 0, 1, 0]);
    let mut deck = TzxDeck::new(TzxImage::parse(&bytes).unwrap());
    assert_eq!(deck.advance(100), 4);
    assert_eq!(deck.status(), PlaybackStatus::Stopped);
    assert!(deck.resume());
    assert_eq!(deck.advance(100), 5);
    assert_eq!(deck.status(), PlaybackStatus::Finished);
    deck.rewind();
    assert_eq!(deck.advance(2), 2);
    assert!(deck.level());
    assert_eq!(deck.remaining(), 2);
}

#[test]
fn metadata_is_retained_and_does_not_change_playback() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x21, 1, b'G', 0x22, 0x30, 1, b'T']);
    bytes.extend_from_slice(&[0x31, 5, 1, b'M', 0x32, 2, 0, 0xaa, 0xbb]);
    bytes.extend_from_slice(&[0x33, 1, 1, 2, 3]);
    bytes.extend_from_slice(&[0x12, 3, 0, 1, 0]);
    let image = TzxImage::parse(&bytes).unwrap();
    assert_eq!(
        image.blocks()[3],
        TzxBlock::Metadata {
            id: 0x31,
            data: vec![5, b'M']
        }
    );
    let mut deck = TzxDeck::new(image);
    pulse(&mut deck, false, 3);
    assert_eq!(deck.status(), PlaybackStatus::Finished);
    bytes.pop();
    assert!(TzxImage::parse(&bytes).is_err());
}

#[test]
fn zero_pause_preserves_polarity_and_nonzero_pause_finishes_low() {
    let mut bytes = header();
    bytes.extend_from_slice(&[0x12, 3, 0, 1, 0]); // end at high
    bytes.extend_from_slice(&[0x14, 4, 0, 5, 0, 1, 0, 0, 1, 0, 0, 0]);
    bytes.extend_from_slice(&[0x20, 1, 0]);
    bytes.extend_from_slice(&[0x12, 6, 0, 1, 0]);
    let mut deck = TzxDeck::new(TzxImage::parse(&bytes).unwrap());
    pulse(&mut deck, false, 3);
    pulse(&mut deck, true, 4);
    pulse(&mut deck, false, 4);
    pulse(&mut deck, true, T_STATES_PER_MS);
    pulse(&mut deck, false, 6);
    assert_eq!(deck.status(), PlaybackStatus::Finished);
}

#[test]
fn rejects_truncation_unsupported_blocks_and_invalid_parameters() {
    let bytes = turbo();
    for len in 0..bytes.len() {
        if len == 10 {
            continue;
        } // empty image is valid
        assert!(TzxImage::parse(&bytes[..len]).is_err(), "length {len}");
    }
    let mut bad = bytes.clone();
    bad[0] = 0;
    assert!(matches!(TzxImage::parse(&bad), Err(Error::InvalidInput(_))));
    bad = bytes.clone();
    bad[8] = 2;
    assert!(matches!(TzxImage::parse(&bad), Err(Error::Unsupported(_))));
    bad = bytes.clone();
    bad[23] = 0;
    assert!(TzxImage::parse(&bad).is_err());
    bad = bytes.clone();
    bad[23] = 9;
    assert!(TzxImage::parse(&bad).is_err());
    bad = bytes.clone();
    bad[11..13].copy_from_slice(&[0, 0]);
    assert!(TzxImage::parse(&bad).is_err());
    let mut unknown = header();
    unknown.push(0x23); // control flow must never be skipped
    assert!(matches!(
        TzxImage::parse(&unknown),
        Err(Error::Unsupported(_))
    ));
    let mut empty_standard = header();
    empty_standard.extend_from_slice(&[0x10, 0, 0, 0, 0]);
    assert!(TzxImage::parse(&empty_standard).is_err());
}

#[test]
fn machine_mounting_is_transactional_and_tape_formats_replace_each_other() {
    let mut machine = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    let tzx = turbo();
    let tap = include_bytes!("fixtures/basic.tap");
    machine
        .load_media(&Media {
            format: "tap",
            bytes: tap,
        })
        .unwrap();
    assert!(machine
        .load_media(&Media {
            format: "tzx",
            bytes: &[0]
        })
        .is_err());
    assert!(machine.tape().is_some());
    machine
        .load_media(&Media {
            format: "tzx",
            bytes: &tzx,
        })
        .unwrap();
    assert!(machine.tape().is_none());
    assert_eq!(machine.tzx_tape().unwrap().remaining(), 10);
    assert_eq!(machine.tzx_tape_mut().unwrap().advance(5), 5);
    assert!(machine
        .load_media(&Media {
            format: "tap",
            bytes: &[0]
        })
        .is_err());
    assert_eq!(machine.tzx_tape().unwrap().remaining(), 5);
    machine
        .load_media(&Media {
            format: "tap",
            bytes: tap,
        })
        .unwrap();
    assert!(machine.tzx_tape().is_none());
    assert!(machine.tape().is_some());
}
