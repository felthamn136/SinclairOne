use sinclair_core::{
    media::{Media, MediaCodec},
    Error, Machine,
};
use sinclair_spectrum48::{
    media::tap::{TapCodec, TapDeck, TapImage},
    memory::{Memory, ROM_SIZE},
    Spectrum48,
};

const BASIC: &[u8] = include_bytes!("fixtures/basic.tap");
const MIXED: &[u8] = include_bytes!("fixtures/mixed.tap");
const PROGRAM: &[u8] = &[0x00, 0x0a, 0x06, 0x00, 0xf5, 0x22, 0x48, 0x49, 0x22, 0x0d];

fn memory() -> Memory {
    Memory::new(&[0; ROM_SIZE]).unwrap()
}

#[test]
fn reads_and_loads_a_known_basic_tap_file() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/basic.tap");
    let image = TapImage::from_file(path).unwrap();
    assert_eq!(image.blocks().len(), 2);
    assert_eq!(image.blocks()[0].flag(), 0);
    assert_eq!(image.blocks()[1].flag(), 0xff);
    assert_eq!(image.blocks()[1].payload(), PROGRAM);
    assert_eq!(TapCodec.decode(BASIC).unwrap(), image);
    let mut deck = TapDeck::new(image);
    let mut memory = memory();
    let header = deck.load_next_basic(&mut memory, 0x8000).unwrap();
    assert_eq!(&header.name, b"HELLO     ");
    assert_eq!(
        (
            header.data_length,
            header.program_length,
            header.autostart_line
        ),
        (10, 10, 10)
    );
    for (offset, value) in PROGRAM.iter().enumerate() {
        assert_eq!(memory.read(0x8000 + offset as u16), *value);
    }
    assert_eq!(deck.position(), 2);
    assert!(matches!(
        deck.load_next_basic(&mut memory, 0x8000),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn verify_matches_and_reports_mismatch_without_modifying_memory_or_cursor() {
    let mut deck = TapDeck::new(TapImage::parse(BASIC).unwrap());
    let mut memory = memory();
    deck.load_next_basic(&mut memory, 0x8000).unwrap();
    deck.rewind();
    assert_eq!(
        deck.verify_next_basic(&memory, 0x8000).unwrap().data_length,
        10
    );
    assert_eq!(deck.position(), 2);
    deck.rewind();
    memory.write(0x8004, 0);
    let error = deck.verify_next_basic(&memory, 0x8000).unwrap_err();
    assert!(error.to_string().contains("8004"));
    assert_eq!(memory.read(0x8004), 0);
    assert_eq!(deck.position(), 0);
}

#[test]
fn empty_name_search_skips_non_basic_headers_in_a_known_mixed_tap() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mixed.tap");
    let image = TapImage::from_file(path).unwrap();
    assert_eq!(image, TapImage::parse(MIXED).unwrap());
    assert_eq!(image.blocks().len(), 4);
    let mut deck = TapDeck::new(image);
    let header = deck.load_next_basic(&mut memory(), 0x4000).unwrap();
    assert_eq!(&header.name, b"HELLO     ");
    assert_eq!(deck.position(), 4);
}

#[test]
fn rejects_truncated_short_and_checksum_invalid_blocks() {
    for invalid in [
        &[0x13][..],
        &[0x00, 0x00][..],
        &[0x01, 0x00, 0xff][..],
        &[0x03, 0x00, 0xff, 0xff][..],
        &[0x02, 0x00, 0xff, 0x00][..],
    ] {
        assert!(matches!(
            TapImage::parse(invalid),
            Err(Error::InvalidInput(_))
        ));
    }
    let mut corrupt = BASIC.to_vec();
    corrupt[4] ^= 1;
    let error = TapImage::parse(&corrupt).unwrap_err();
    assert!(error.to_string().contains("checksum"));
    let mut trailing = BASIC.to_vec();
    trailing.push(1);
    assert!(matches!(
        TapImage::parse(&trailing),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn rejects_wrong_basic_data_length_missing_data_and_out_of_ram_range() {
    let header_only = TapImage::parse(&BASIC[..21]).unwrap();
    let mut deck = TapDeck::new(header_only);
    let mut memory = memory();
    assert!(deck.load_next_basic(&mut memory, 0x8000).is_err());
    assert_eq!(deck.position(), 0);

    let mut wrong_length = BASIC.to_vec();
    wrong_length[14] = 11;
    wrong_length[20] = wrong_length[2..20].iter().fold(0, |acc, &b| acc ^ b);
    let mut deck = TapDeck::new(TapImage::parse(&wrong_length).unwrap());
    assert!(deck.load_next_basic(&mut memory, 0x8000).is_err());
    assert_eq!(deck.position(), 0);
    assert_eq!(memory.read(0x8000), 0);

    let mut deck = TapDeck::new(TapImage::parse(BASIC).unwrap());
    for address in [0x3fff, 0xfff7] {
        assert!(deck.load_next_basic(&mut memory, address).is_err());
        assert_eq!(deck.position(), 0);
    }
    deck.load_next_basic(&mut memory, 0xfff6).unwrap();
    assert_eq!(memory.read(0xffff), PROGRAM[9]);
}

#[test]
fn rejects_non_data_block_and_inconsistent_program_length() {
    let mut wrong_flag = BASIC.to_vec();
    wrong_flag[23] = 0xfe;
    let last = wrong_flag.len() - 1;
    wrong_flag[last] = wrong_flag[23..last].iter().fold(0, |acc, &b| acc ^ b);
    let mut deck = TapDeck::new(TapImage::parse(&wrong_flag).unwrap());
    assert!(deck.load_next_basic(&mut memory(), 0x8000).is_err());
    assert_eq!(deck.position(), 0);

    let mut wrong_program_length = BASIC.to_vec();
    wrong_program_length[18] = 11;
    wrong_program_length[20] = wrong_program_length[2..20]
        .iter()
        .fold(0, |acc, &b| acc ^ b);
    let mut deck = TapDeck::new(TapImage::parse(&wrong_program_length).unwrap());
    assert!(deck.load_next_basic(&mut memory(), 0x8000).is_err());
    assert_eq!(deck.position(), 0);
}

#[test]
fn sequential_programs_advance_then_rewind() {
    let mut bytes = BASIC.to_vec();
    bytes.extend_from_slice(BASIC);
    let mut deck = TapDeck::new(TapImage::parse(&bytes).unwrap());
    let mut memory = memory();
    deck.load_next_basic(&mut memory, 0x8000).unwrap();
    assert_eq!(deck.position(), 2);
    deck.verify_next_basic(&memory, 0x8000).unwrap();
    assert_eq!(deck.position(), 4);
    deck.rewind();
    assert_eq!(deck.position(), 0);
    deck.verify_next_basic(&memory, 0x8000).unwrap();
}

#[test]
fn machine_mounts_valid_tap_and_does_not_replace_it_on_failure() {
    let mut machine = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    machine
        .load_media(&Media {
            format: "tap",
            bytes: BASIC,
        })
        .unwrap();
    assert_eq!(machine.tape().unwrap().image().blocks().len(), 2);
    assert!(machine
        .load_media(&Media {
            format: "tap",
            bytes: &[1]
        })
        .is_err());
    assert_eq!(machine.tape().unwrap().image().blocks().len(), 2);
    assert!(matches!(
        machine.load_media(&Media {
            format: "tzx",
            bytes: BASIC
        }),
        Err(Error::InvalidInput(_))
    ));
}
