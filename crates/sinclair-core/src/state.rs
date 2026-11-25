use std::collections::VecDeque;

use crate::{Error, Frame, Machine, Result};

/// An in-memory envelope; persistent encoding is a host concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub machine_id: String,
    pub version: u32,
    pub payload: Vec<u8>,
}

impl State {
    /// Wrap a machine payload with a CRC-32 for accidental corruption detection.
    pub fn sealed(machine_id: &str, version: u32, mut payload: Vec<u8>) -> Self {
        payload.extend_from_slice(&crc32(&payload).to_le_bytes());
        Self {
            machine_id: machine_id.into(),
            version,
            payload,
        }
    }

    pub fn validate_for(&self, machine_id: &str, version: u32) -> Result<()> {
        if self.machine_id != machine_id || self.version != version {
            return Err(Error::IncompatibleState(format!(
                "expected {machine_id} version {version}, got {} version {}",
                self.machine_id, self.version
            )));
        }
        Ok(())
    }

    pub fn checked_payload(&self, machine_id: &str, version: u32) -> Result<&[u8]> {
        self.validate_for(machine_id, version)?;
        let length = self
            .payload
            .len()
            .checked_sub(4)
            .ok_or_else(|| Error::IncompatibleState("truncated state checksum".into()))?;
        let (payload, checksum) = self.payload.split_at(length);
        let expected = u32::from_le_bytes(checksum.try_into().unwrap());
        if crc32(payload) != expected {
            return Err(Error::IncompatibleState("state checksum mismatch".into()));
        }
        Ok(payload)
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

/// Machine-neutral quick slot and bounded history. Capture before advancing.
#[derive(Debug)]
pub struct StateManager {
    quick: Option<State>,
    history: VecDeque<State>,
    history_length: usize,
}

impl StateManager {
    pub fn new(history_length: usize) -> Self {
        Self {
            quick: None,
            history: VecDeque::new(),
            history_length,
        }
    }

    pub fn history_length(&self) -> usize {
        self.history_length
    }

    pub fn set_history_length(&mut self, length: usize) {
        self.history_length = length;
        while self.history.len() > length {
            self.history.pop_front();
        }
    }

    pub fn recorded(&self) -> usize {
        self.history.len()
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn quick_save(&mut self, machine: &dyn Machine) -> Result<()> {
        self.quick = Some(machine.save_state()?);
        Ok(())
    }

    pub fn quick_load(&mut self, machine: &mut dyn Machine) -> Result<()> {
        let state = self
            .quick
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("no quick save is available".into()))?;
        machine.load_state(state)?;
        self.history.clear();
        Ok(())
    }

    /// Record the current state before advancing the machine.
    pub fn record(&mut self, machine: &dyn Machine) -> Result<()> {
        if self.history_length == 0 {
            return Ok(());
        }
        let state = machine.save_state()?;
        self.validate_checkpoint(&state)?;
        self.push_checkpoint(state);
        Ok(())
    }

    /// Run a frame and record its starting state only if the frame succeeds.
    pub fn run_frame<'a>(&mut self, machine: &'a mut dyn Machine) -> Result<Frame<'a>> {
        if self.history_length == 0 {
            return machine.run_frame();
        }
        let state = machine.save_state()?;
        self.validate_checkpoint(&state)?;
        let frame = machine.run_frame()?;
        self.push_checkpoint(state);
        Ok(frame)
    }

    fn validate_checkpoint(&self, state: &State) -> Result<()> {
        if let Some(previous) = self.history.back() {
            state.validate_for(&previous.machine_id, previous.version)?;
        }
        Ok(())
    }

    fn push_checkpoint(&mut self, state: State) {
        if self.history.len() == self.history_length {
            self.history.pop_front();
        }
        self.history.push_back(state);
    }

    /// Restore and consume the most recent checkpoint; failures retain history.
    pub fn rewind(&mut self, machine: &mut dyn Machine) -> Result<()> {
        let state = self
            .history
            .back()
            .ok_or_else(|| Error::InvalidInput("no rewind history is available".into()))?;
        machine.load_state(state)?;
        self.history.pop_back();
        Ok(())
    }
}

/// Length-delimited primitives for machine-owned state payloads.
#[derive(Default)]
pub struct StateWriter {
    bytes: Vec<u8>,
}

impl StateWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let length = u32::try_from(bytes.len())
            .map_err(|_| Error::InvalidInput("state field is too large".into()))?;
        self.u32(length);
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

pub struct StateReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> StateReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        if length > self.bytes.len() - self.offset {
            return Err(Error::IncompatibleState("truncated state payload".into()));
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::IncompatibleState("invalid state boolean".into())),
        }
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| Error::IncompatibleState("state field is too large".into()))?;
        self.take(length)
    }
    pub fn finish(self) -> Result<()> {
        if self.offset != self.bytes.len() {
            return Err(Error::IncompatibleState("trailing state data".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_requires_matching_machine_and_version() {
        let state = State {
            machine_id: "spectrum48".into(),
            version: 1,
            payload: vec![1],
        };
        assert!(state.validate_for("spectrum48", 1).is_ok());
        assert!(matches!(
            state.validate_for("zx81", 1),
            Err(Error::IncompatibleState(_))
        ));
        assert!(matches!(
            state.validate_for("spectrum48", 2),
            Err(Error::IncompatibleState(_))
        ));
    }

    #[test]
    fn sealed_state_detects_corruption() {
        let state = State::sealed("test", 1, b"123456789".to_vec());
        assert_eq!(&state.payload[9..], &0xcbf4_3926u32.to_le_bytes());
        assert_eq!(state.checked_payload("test", 1).unwrap(), b"123456789");
        let mut corrupt = state.clone();
        corrupt.payload[0] ^= 1;
        assert!(corrupt.checked_payload("test", 1).is_err());
        corrupt.payload.truncate(3);
        assert!(corrupt.checked_payload("test", 1).is_err());
    }
}
