use sinclair_core::{services::input::InputEvent, Error, Machine};
use sinclair_spectrum48::{
    keyboard::{Key, KeyboardMatrix},
    memory::ROM_SIZE,
    Spectrum48,
};

#[test]
fn each_of_the_forty_keys_scans_on_its_row_and_column() {
    let rows = [
        [Key::CapsShift, Key::Z, Key::X, Key::C, Key::V],
        [Key::A, Key::S, Key::D, Key::F, Key::G],
        [Key::Q, Key::W, Key::E, Key::R, Key::T],
        [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5],
        [Key::Num0, Key::Num9, Key::Num8, Key::Num7, Key::Num6],
        [Key::P, Key::O, Key::I, Key::U, Key::Y],
        [Key::Enter, Key::L, Key::K, Key::J, Key::H],
        [Key::Space, Key::SymbolShift, Key::M, Key::N, Key::B],
    ];
    let mut keyboard = KeyboardMatrix::new();
    for (row, keys) in rows.iter().enumerate() {
        let port = 0xfffe & !(1 << (row + 8));
        for (column, &key) in keys.iter().enumerate() {
            keyboard.set_key(key, true);
            assert_eq!(keyboard.column_mask(port), 0x1f & !(1 << column));
            assert_eq!(keyboard.column_mask(0xfffe), 0x1f);
            keyboard.set_key(key, false);
            assert_eq!(keyboard.column_mask(port), 0x1f);
        }
    }
}

#[test]
fn simultaneous_rows_combine_and_release_independently() {
    let mut keyboard = KeyboardMatrix::new();
    keyboard.set_key(Key::A, true);
    keyboard.set_key(Key::M, true);
    assert_eq!(keyboard.column_mask(0xfdfe), 0x1e);
    assert_eq!(keyboard.column_mask(0x7ffe), 0x1b);
    assert_eq!(keyboard.column_mask(0x7dfe), 0x1a);
    keyboard.set_key(Key::A, false);
    assert_eq!(keyboard.column_mask(0x7dfe), 0x1b);
    keyboard.release_all();
    assert_eq!(keyboard.column_mask(0x00fe), 0x1f);
}

#[test]
fn caps_and_symbol_shifts_are_independent_matrix_keys() {
    let mut keyboard = KeyboardMatrix::new();
    keyboard.set_key(Key::CapsShift, true);
    keyboard.set_key(Key::SymbolShift, true);
    keyboard.set_key(Key::A, true);
    keyboard.set_key(Key::P, true);
    assert_eq!(keyboard.column_mask(0xfefe), 0x1e);
    assert_eq!(keyboard.column_mask(0x7ffe), 0x1d);
    assert_eq!(keyboard.column_mask(0xfdfe), 0x1e);
    assert_eq!(keyboard.column_mask(0xdffe), 0x1e);
    assert_eq!(keyboard.column_mask(0x7efe), 0x1c);
}

#[test]
fn machine_input_updates_keyboard_and_rejects_unknown_controls() {
    let mut machine = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    machine
        .input(InputEvent {
            control: "caps_shift".into(),
            pressed: true,
        })
        .unwrap();
    assert_eq!(machine.keyboard().column_mask(0xfefe), 0x1e);
    assert!(matches!(
        machine.input(InputEvent {
            control: "not_a_key".into(),
            pressed: true
        }),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(machine.keyboard().column_mask(0xfefe), 0x1e);
    machine
        .input(InputEvent {
            control: "caps_shift".into(),
            pressed: false,
        })
        .unwrap();
    assert_eq!(machine.keyboard().column_mask(0xfefe), 0x1f);
}
