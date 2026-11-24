//! Standard Spectrum TAP blocks and BASIC program transfers.
//!
//! Parsing and direct transfers do not simulate tape pulses or invoke the ROM.

use std::path::Path;

use sinclair_core::{media::MediaCodec, Error, Result};

use crate::memory::{Memory, ADDRESS_SPACE_SIZE, ROM_SIZE};

const HEADER_FLAG: u8 = 0x00;
const DATA_FLAG: u8 = 0xff;
const STANDARD_HEADER_LEN: usize = 17;

/// One validated TAP block, including its flag and checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapBlock {
    bytes: Vec<u8>,
}

impl TapBlock {
    pub fn flag(&self) -> u8 {
        self.bytes[0]
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes[1..self.bytes.len() - 1]
    }

    pub fn checksum(&self) -> u8 {
        self.bytes[self.bytes.len() - 1]
    }
}

/// Parsed TAP blocks, in tape order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapImage {
    blocks: Vec<TapBlock>,
}

impl TapImage {
    /// Validate every length and checksum before returning an image.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut blocks = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            if bytes.len() - offset < 2 {
                return Err(Error::InvalidInput(format!(
                    "truncated TAP length at offset {offset}"
                )));
            }
            let length = usize::from(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
            let block_offset = offset + 2;
            if length < 2 {
                return Err(Error::InvalidInput(format!(
                    "TAP block {} at offset {offset} has invalid length {length}",
                    blocks.len()
                )));
            }
            if length > bytes.len() - block_offset {
                return Err(Error::InvalidInput(format!(
                    "TAP block {} at offset {offset} declares {length} bytes but only {} remain",
                    blocks.len(),
                    bytes.len() - block_offset
                )));
            }
            let end = block_offset + length;
            let block = &bytes[block_offset..end];
            if block.iter().fold(0u8, |xor, byte| xor ^ byte) != 0 {
                return Err(Error::InvalidInput(format!(
                    "TAP block {} at offset {offset} has a checksum mismatch",
                    blocks.len()
                )));
            }
            blocks.push(TapBlock {
                bytes: block.to_vec(),
            });
            offset = end;
        }
        Ok(Self { blocks })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn blocks(&self) -> &[TapBlock] {
        &self.blocks
    }

    fn next_basic(&self, start: usize) -> Result<(BasicHeader, &[u8], usize)> {
        for index in start..self.blocks.len() {
            let block = &self.blocks[index];
            let payload = block.payload();
            if block.flag() != HEADER_FLAG
                || payload.len() != STANDARD_HEADER_LEN
                || payload[0] != 0
            {
                continue;
            }
            let header = BasicHeader::parse(payload);
            if header.program_length > header.data_length {
                return Err(Error::InvalidInput(format!(
                    "TAP BASIC header at block {index} has program length greater than data length"
                )));
            }
            let data = self.blocks.get(index + 1).ok_or_else(|| {
                Error::InvalidInput(format!(
                    "TAP BASIC header at block {index} has no data block"
                ))
            })?;
            if data.flag() != DATA_FLAG {
                return Err(Error::InvalidInput(format!(
                    "TAP BASIC header at block {index} is not followed by a standard data block"
                )));
            }
            if data.payload().len() != usize::from(header.data_length) {
                return Err(Error::InvalidInput(format!(
                    "TAP BASIC data at block {} has length {}, expected {}",
                    index + 1,
                    data.payload().len(),
                    header.data_length
                )));
            }
            return Ok((header, data.payload(), index + 2));
        }
        Err(Error::InvalidInput(
            "no further standard BASIC program found in TAP".into(),
        ))
    }
}

pub struct TapCodec;

impl MediaCodec for TapCodec {
    type Output = TapImage;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output> {
        TapImage::parse(bytes)
    }
}

/// Raw Spectrum name bytes and the fields of a standard BASIC header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicHeader {
    pub name: [u8; 10],
    pub data_length: u16,
    pub autostart_line: u16,
    pub program_length: u16,
}

impl BasicHeader {
    fn parse(payload: &[u8]) -> Self {
        let mut name = [0; 10];
        name.copy_from_slice(&payload[1..11]);
        Self {
            name,
            data_length: u16::from_le_bytes([payload[11], payload[12]]),
            autostart_line: u16::from_le_bytes([payload[13], payload[14]]),
            program_length: u16::from_le_bytes([payload[15], payload[16]]),
        }
    }
}

/// A sequential cursor for direct standard BASIC program load and verify.
#[derive(Debug, Clone)]
pub struct TapDeck {
    image: TapImage,
    position: usize,
}

impl TapDeck {
    pub fn new(image: TapImage) -> Self {
        Self { image, position: 0 }
    }

    pub fn image(&self) -> &TapImage {
        &self.image
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn rewind(&mut self) {
        self.position = 0;
    }

    /// Copy the next standard BASIC block into RAM at the caller's address.
    /// This does not update the ROM's BASIC workspace or start the program.
    pub fn load_next_basic(&mut self, memory: &mut Memory, address: u16) -> Result<BasicHeader> {
        let (header, bytes, next) = self.image.next_basic(self.position)?;
        validate_range(address, bytes.len())?;
        for (offset, &value) in bytes.iter().enumerate() {
            memory.write((usize::from(address) + offset) as u16, value);
        }
        self.position = next;
        Ok(header)
    }

    /// Compare the next standard BASIC block with RAM, without changing RAM.
    pub fn verify_next_basic(&mut self, memory: &Memory, address: u16) -> Result<BasicHeader> {
        let (header, bytes, next) = self.image.next_basic(self.position)?;
        validate_range(address, bytes.len())?;
        for (offset, &expected) in bytes.iter().enumerate() {
            let at = (usize::from(address) + offset) as u16;
            let actual = memory.read(at);
            if actual != expected {
                return Err(Error::InvalidInput(format!(
                    "TAP verification failed at {at:04x}: expected {expected:02x}, got {actual:02x}"
                )));
            }
        }
        self.position = next;
        Ok(header)
    }
}

fn validate_range(address: u16, length: usize) -> Result<()> {
    let start = usize::from(address);
    if start < ROM_SIZE || length > ADDRESS_SPACE_SIZE - start {
        return Err(Error::InvalidInput(format!(
            "TAP data at {address:04x} with length {length} does not fit in RAM"
        )));
    }
    Ok(())
}
