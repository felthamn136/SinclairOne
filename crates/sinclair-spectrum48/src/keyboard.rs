//! Spectrum 48K keyboard matrix and keyboard port decoding.

use sinclair_core::{services::input::InputEvent, Error, Result};

use crate::{bus::SpectrumIo, memory::Memory};

/// Keys are ordered by matrix row (A8 through A15), then column (D0 through D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Key {
    CapsShift,
    Z,
    X,
    C,
    V,
    A,
    S,
    D,
    F,
    G,
    Q,
    W,
    E,
    R,
    T,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num0,
    Num9,
    Num8,
    Num7,
    Num6,
    P,
    O,
    I,
    U,
    Y,
    Enter,
    L,
    K,
    J,
    H,
    Space,
    SymbolShift,
    M,
    N,
    B,
}

impl Key {
    pub const ALL: [Self; 40] = [
        Self::CapsShift,
        Self::Z,
        Self::X,
        Self::C,
        Self::V,
        Self::A,
        Self::S,
        Self::D,
        Self::F,
        Self::G,
        Self::Q,
        Self::W,
        Self::E,
        Self::R,
        Self::T,
        Self::Num1,
        Self::Num2,
        Self::Num3,
        Self::Num4,
        Self::Num5,
        Self::Num0,
        Self::Num9,
        Self::Num8,
        Self::Num7,
        Self::Num6,
        Self::P,
        Self::O,
        Self::I,
        Self::U,
        Self::Y,
        Self::Enter,
        Self::L,
        Self::K,
        Self::J,
        Self::H,
        Self::Space,
        Self::SymbolShift,
        Self::M,
        Self::N,
        Self::B,
    ];

    pub fn control_id(self) -> &'static str {
        const IDS: [&str; 40] = [
            "caps_shift",
            "z",
            "x",
            "c",
            "v",
            "a",
            "s",
            "d",
            "f",
            "g",
            "q",
            "w",
            "e",
            "r",
            "t",
            "1",
            "2",
            "3",
            "4",
            "5",
            "0",
            "9",
            "8",
            "7",
            "6",
            "p",
            "o",
            "i",
            "u",
            "y",
            "enter",
            "l",
            "k",
            "j",
            "h",
            "space",
            "symbol_shift",
            "m",
            "n",
            "b",
        ];
        IDS[self as usize]
    }

    pub fn from_control_id(control: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|key| key.control_id() == control)
            .ok_or_else(|| Error::InvalidInput(format!("unknown Spectrum key: {control}")))
    }
}

/// Each low bit denotes a pressed key; unselected rows do not affect a read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardMatrix {
    rows: [u8; 8],
}

impl Default for KeyboardMatrix {
    fn default() -> Self {
        Self { rows: [0x1f; 8] }
    }
}

impl KeyboardMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_key(&mut self, key: Key, pressed: bool) {
        let index = key as usize;
        let mask = 1 << (index % 5);
        if pressed {
            self.rows[index / 5] &= !mask;
        } else {
            self.rows[index / 5] |= mask;
        }
    }

    pub fn apply_input(&mut self, event: &InputEvent) -> Result<()> {
        self.set_key(Key::from_control_id(&event.control)?, event.pressed);
        Ok(())
    }

    pub fn release_all(&mut self) {
        self.rows = [0x1f; 8];
    }

    /// Return the active-low D0–D4 mask for rows selected by A8–A15.
    pub fn column_mask(&self, port: u16) -> u8 {
        let mut columns = 0x1f;
        for (row, &value) in self.rows.iter().enumerate() {
            if port & (1 << (8 + row)) == 0 {
                columns &= value;
            }
        }
        columns
    }
}

/// Add keyboard scanning to a machine's existing port and interrupt device.
pub struct KeyboardIo<'a, I: SpectrumIo + ?Sized> {
    keyboard: &'a KeyboardMatrix,
    io: &'a mut I,
}

impl<'a, I: SpectrumIo + ?Sized> KeyboardIo<'a, I> {
    pub fn new(keyboard: &'a KeyboardMatrix, io: &'a mut I) -> Self {
        Self { keyboard, io }
    }
}

impl<I: SpectrumIo + ?Sized> SpectrumIo for KeyboardIo<'_, I> {
    fn read_port(&mut self, port: u16) -> u8 {
        let value = self.io.read_port(port);
        if port & 1 == 0 {
            value & (0xe0 | self.keyboard.column_mask(port))
        } else {
            value
        }
    }

    fn write_port(&mut self, port: u16, value: u8) {
        self.io.write_port(port, value);
    }

    fn interrupt_acknowledge(&mut self) -> u8 {
        self.io.interrupt_acknowledge()
    }

    fn interrupt_data(&mut self, address: u16, memory: &mut Memory) -> u8 {
        self.io.interrupt_data(address, memory)
    }

    fn nmi_acknowledge(&mut self) {
        self.io.nmi_acknowledge();
    }

    fn reti(&mut self) {
        self.io.reti();
    }
}
