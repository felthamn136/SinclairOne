use sinclair_core::{Error, Result};

pub const ROM_SIZE: usize = 8 * 1024;
pub const RAM_START: u16 = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSize {
    OneK,
    SixteenK,
}

impl RamSize {
    pub fn bytes(self) -> usize {
        match self {
            Self::OneK => 1024,
            Self::SixteenK => 16 * 1024,
        }
    }
}

/// Stock ZX81 address decoding with either 1K or a 16K RAM pack.
pub struct Memory {
    rom: Box<[u8]>,
    ram: Box<[u8]>,
}

impl Memory {
    pub fn new(rom: &[u8], ram_size: RamSize) -> Result<Self> {
        if rom.len() != ROM_SIZE {
            return Err(Error::InvalidInput(format!(
                "ZX81 ROM must be {ROM_SIZE} bytes, got {}",
                rom.len()
            )));
        }
        Ok(Self {
            rom: rom.into(),
            ram: vec![0; ram_size.bytes()].into_boxed_slice(),
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        let address = usize::from(address & 0x7fff);
        if address < usize::from(RAM_START) {
            self.rom[address % ROM_SIZE]
        } else {
            self.ram[(address - usize::from(RAM_START)) % self.ram.len()]
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        let address = usize::from(address & 0x7fff);
        if address >= usize::from(RAM_START) {
            self.ram[(address - usize::from(RAM_START)) % self.ram.len()] = value;
        }
    }

    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    pub fn load_ram(&mut self, ram: &[u8]) -> Result<()> {
        if ram.len() != self.ram.len() {
            return Err(Error::IncompatibleState("invalid ZX81 RAM size".into()));
        }
        self.ram.copy_from_slice(ram);
        Ok(())
    }
}
