use sinclair_core::{
    state::{StateReader, StateWriter},
    Error, Result,
};
use sinclair_spectrum48::video::{ScreenMemory, SCREEN_BYTES};

pub const BANK_SIZE: usize = 16 * 1024;
pub const RAM_BANKS: usize = 8;
pub const ROM_BANKS: usize = 2;

/// Original Spectrum 128K memory map and 7FFD paging latch.
pub struct Memory {
    rom: [Box<[u8]>; ROM_BANKS],
    ram: [Box<[u8; BANK_SIZE]>; RAM_BANKS],
    paging: u8,
}

impl Memory {
    /// ROM 0 is 128K BASIC; ROM 1 is 48K BASIC.
    pub fn new(rom0: &[u8], rom1: &[u8]) -> Result<Self> {
        for (index, rom) in [rom0, rom1].into_iter().enumerate() {
            if rom.len() != BANK_SIZE {
                return Err(Error::InvalidInput(format!(
                    "Spectrum 128K ROM {index} must be {BANK_SIZE} bytes, got {}",
                    rom.len()
                )));
            }
        }
        Ok(Self {
            rom: [rom0.into(), rom1.into()],
            ram: std::array::from_fn(|_| Box::new([0; BANK_SIZE])),
            paging: 0,
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        let offset = usize::from(address) & (BANK_SIZE - 1);
        match address >> 14 {
            0 => self.rom[self.selected_rom()][offset],
            1 => self.ram[5][offset],
            2 => self.ram[2][offset],
            _ => self.ram[self.selected_bank()][offset],
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        let offset = usize::from(address) & (BANK_SIZE - 1);
        let bank = match address >> 14 {
            0 => return,
            1 => 5,
            2 => 2,
            _ => self.selected_bank(),
        };
        self.ram[bank][offset] = value;
    }

    /// Write to a partially decoded 7FFD port; unrelated ports are ignored.
    pub fn write_port(&mut self, port: u16, value: u8) {
        if port & 0x8002 == 0 && !self.paging_locked() {
            self.paging = value;
        }
    }

    /// Reset the paging latch without erasing RAM.
    pub fn reset_paging(&mut self) {
        self.paging = 0;
    }

    pub fn paging_register(&self) -> u8 {
        self.paging
    }

    pub fn selected_bank(&self) -> usize {
        usize::from(self.paging & 7)
    }

    pub fn selected_rom(&self) -> usize {
        usize::from((self.paging >> 4) & 1)
    }

    pub fn screen_bank(&self) -> usize {
        if self.paging & 8 == 0 {
            5
        } else {
            7
        }
    }

    pub fn paging_locked(&self) -> bool {
        self.paging & 0x20 != 0
    }

    pub fn write_state(&self, writer: &mut StateWriter) -> Result<()> {
        for rom in &self.rom {
            writer.bytes(rom)?;
        }
        for bank in &self.ram {
            writer.bytes(&bank[..])?;
        }
        writer.u8(self.paging);
        Ok(())
    }

    /// Decode RAM and latch only after verifying the installed ROMs.
    pub fn read_state(&self, reader: &mut StateReader<'_>) -> Result<Self> {
        for rom in &self.rom {
            if reader.bytes()? != &rom[..] {
                return Err(Error::IncompatibleState(
                    "Spectrum 128K ROM mismatch".into(),
                ));
            }
        }
        let mut ram: [Box<[u8; BANK_SIZE]>; RAM_BANKS] =
            std::array::from_fn(|_| Box::new([0; BANK_SIZE]));
        for bank in &mut ram {
            let bytes = reader.bytes()?;
            if bytes.len() != BANK_SIZE {
                return Err(Error::IncompatibleState(
                    "invalid Spectrum 128K RAM bank".into(),
                ));
            }
            bank.copy_from_slice(bytes);
        }
        let paging = reader.u8()?;
        Ok(Self {
            rom: self.rom.clone(),
            ram,
            paging,
        })
    }
}

impl ScreenMemory for Memory {
    fn screen_ram(&self) -> &[u8] {
        &self.ram[self.screen_bank()][..SCREEN_BYTES]
    }
}
