use sinclair_core::{Error, Result};

pub const ROM_SIZE: usize = 16 * 1024;
pub const RAM_SIZE: usize = 48 * 1024;
pub const ADDRESS_SPACE_SIZE: usize = ROM_SIZE + RAM_SIZE;

/// A flat 64 KiB address map with a write-protected 16 KiB ROM.
pub struct Memory {
    bytes: Box<[u8]>,
}

impl Memory {
    pub fn new(rom: &[u8]) -> Result<Self> {
        if rom.len() != ROM_SIZE {
            return Err(Error::InvalidInput(format!(
                "Spectrum 48K ROM must be {ROM_SIZE} bytes, got {}",
                rom.len()
            )));
        }
        let mut bytes = vec![0; ADDRESS_SPACE_SIZE].into_boxed_slice();
        bytes[..ROM_SIZE].copy_from_slice(rom);
        Ok(Self { bytes })
    }

    #[inline]
    pub fn read(&self, address: u16) -> u8 {
        self.bytes[usize::from(address)]
    }

    pub fn ram(&self) -> &[u8] {
        &self.bytes[ROM_SIZE..]
    }

    pub fn rom(&self) -> &[u8] {
        &self.bytes[..ROM_SIZE]
    }

    /// Replace RAM without changing the ROM.
    pub fn load_ram(&mut self, ram: &[u8]) -> Result<()> {
        if ram.len() != RAM_SIZE {
            return Err(Error::InvalidInput(format!(
                "Spectrum 48K RAM must be {RAM_SIZE} bytes, got {}",
                ram.len()
            )));
        }
        self.bytes[ROM_SIZE..].copy_from_slice(ram);
        Ok(())
    }

    /// Writes to the ROM address range are ignored, as on the 48K hardware.
    #[inline]
    pub fn write(&mut self, address: u16, value: u8) {
        let address = usize::from(address);
        if address >= ROM_SIZE {
            self.bytes[address] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_rom_size() {
        assert!(matches!(Memory::new(&[]), Err(Error::InvalidInput(_))));
        assert!(Memory::new(&[0; ROM_SIZE]).is_ok());
        assert!(matches!(
            Memory::new(&[0; ROM_SIZE + 1]),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn maps_rom_and_ram_at_boundaries() {
        let mut memory = Memory::new(&[0xa5; ROM_SIZE]).unwrap();
        for address in [0, 0x3fff] {
            memory.write(address, 0x11);
            assert_eq!(memory.read(address), 0xa5);
        }
        for address in [0x4000, 0xffff] {
            memory.write(address, 0x42);
            assert_eq!(memory.read(address), 0x42);
        }
    }

    #[test]
    fn all_addresses_are_independent_and_rom_is_protected() {
        let rom: Vec<u8> = (0..ROM_SIZE).map(|i| (i >> 5) as u8).collect();
        let mut memory = Memory::new(&rom).unwrap();
        for address in 0x4000..=u16::MAX {
            assert_eq!(memory.read(address), 0);
        }
        for address in 0..=u16::MAX {
            let value = (address as u8) ^ ((address >> 8) as u8).wrapping_mul(37);
            memory.write(address, value);
        }
        for address in 0..=u16::MAX {
            let expected = if usize::from(address) < ROM_SIZE {
                rom[usize::from(address)]
            } else {
                (address as u8) ^ ((address >> 8) as u8).wrapping_mul(37)
            };
            assert_eq!(memory.read(address), expected, "address {address:04x}");
        }
    }
}
