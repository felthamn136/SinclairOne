use sinclair_core::{media::MediaCodec, Error, Result};

use crate::memory::{Memory, RAM_START};

pub const LOAD_ADDRESS: u16 = 0x4009;
const SYSTEM_VARIABLE_BYTES: usize = 0x407d - 0x4009;
const E_LINE_OFFSET: usize = 0x4014 - 0x4009;

/// A headerless ZX81 .p image, without a cassette filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    bytes: Vec<u8>,
}

impl Program {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < SYSTEM_VARIABLE_BYTES {
            return Err(Error::InvalidInput("truncated ZX81 program image".into()));
        }
        let end = usize::from(u16::from_le_bytes([
            bytes[E_LINE_OFFSET],
            bytes[E_LINE_OFFSET + 1],
        ]));
        if end != usize::from(LOAD_ADDRESS) + bytes.len() || end > 0x8000 {
            return Err(Error::InvalidInput("invalid ZX81 program length".into()));
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Copy the saved RAM range, without simulating the ROM cassette loader.
    pub fn copy_to_memory(&self, memory: &mut Memory) -> Result<()> {
        let end = usize::from(LOAD_ADDRESS) + self.bytes.len();
        if end > usize::from(RAM_START) + memory.ram().len() {
            return Err(Error::InvalidInput(
                "ZX81 program exceeds installed RAM".into(),
            ));
        }
        for (index, &byte) in self.bytes.iter().enumerate() {
            memory.write(LOAD_ADDRESS + index as u16, byte);
        }
        Ok(())
    }
}

pub struct ProgramCodec;

impl MediaCodec for ProgramCodec {
    type Output = Program;

    fn decode(&self, bytes: &[u8]) -> Result<Program> {
        Program::parse(bytes)
    }
}
