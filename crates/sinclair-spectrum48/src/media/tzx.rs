//! TZX block parsing and streaming pulse playback.

use std::path::Path;

use sinclair_core::{
    media::MediaCodec,
    state::{StateReader, StateWriter},
    Error, Result,
};

pub const T_STATES_PER_MS: u64 = 3_500;
const SIGNATURE: &[u8; 8] = b"ZXTape!\x1a";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataBlock {
    pub pilot_pulse: u16,
    pub pilot_count: u16,
    pub sync1: u16,
    pub sync2: u16,
    pub zero_pulse: u16,
    pub one_pulse: u16,
    pub used_bits: u8,
    pub pause_ms: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TzxBlock {
    Standard(DataBlock),
    Turbo(DataBlock),
    PureData(DataBlock),
    Tone {
        pulse: u16,
        count: u16,
    },
    Pulses(Vec<u16>),
    Pause(u16),
    SetLevel(bool),
    /// Non-playing text, group, archive or hardware information.
    Metadata {
        id: u8,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TzxImage {
    version: (u8, u8),
    blocks: Vec<TzxBlock>,
    bytes: Vec<u8>,
}

impl TzxImage {
    /// Parse and validate the entire image before playback.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader { bytes, offset: 0 };
        if reader.take(8)? != SIGNATURE {
            return Err(Error::InvalidInput("invalid TZX signature".into()));
        }
        let major = reader.byte()?;
        let minor = reader.byte()?;
        if major != 1 {
            return Err(Error::Unsupported("TZX major version"));
        }
        let mut blocks = Vec::new();
        while reader.offset < bytes.len() {
            let id = reader.byte()?;
            let block = match id {
                0x10 => {
                    let pause_ms = reader.word()?;
                    let length = usize::from(reader.word()?);
                    let data = reader.take(length)?.to_vec();
                    if data.is_empty() {
                        return Err(Error::InvalidInput("empty standard TZX data block".into()));
                    }
                    let pilot_count = if data[0] < 128 { 8063 } else { 3223 };
                    TzxBlock::Standard(DataBlock {
                        pilot_pulse: 2168,
                        pilot_count,
                        sync1: 667,
                        sync2: 735,
                        zero_pulse: 855,
                        one_pulse: 1710,
                        used_bits: 8,
                        pause_ms,
                        data,
                    })
                }
                0x11 => {
                    let pilot_pulse = reader.word()?;
                    let sync1 = reader.word()?;
                    let sync2 = reader.word()?;
                    let zero_pulse = reader.word()?;
                    let one_pulse = reader.word()?;
                    let pilot_count = reader.word()?;
                    let used_bits = reader.byte()?;
                    let pause_ms = reader.word()?;
                    let length = reader.length24()?;
                    let data = reader.take(length)?.to_vec();
                    validate_data(&data, used_bits, zero_pulse, one_pulse)?;
                    if sync1 == 0 || sync2 == 0 || (pilot_count != 0 && pilot_pulse == 0) {
                        return Err(Error::InvalidInput("zero TZX pulse length".into()));
                    }
                    TzxBlock::Turbo(DataBlock {
                        pilot_pulse,
                        pilot_count,
                        sync1,
                        sync2,
                        zero_pulse,
                        one_pulse,
                        used_bits,
                        pause_ms,
                        data,
                    })
                }
                0x12 => {
                    let pulse = reader.word()?;
                    let count = reader.word()?;
                    if count != 0 && pulse == 0 {
                        return Err(Error::InvalidInput("zero TZX pulse length".into()));
                    }
                    TzxBlock::Tone { pulse, count }
                }
                0x13 => {
                    let count = usize::from(reader.byte()?);
                    let mut pulses = Vec::with_capacity(count);
                    for _ in 0..count {
                        let pulse = reader.word()?;
                        if pulse == 0 {
                            return Err(Error::InvalidInput("zero TZX pulse length".into()));
                        }
                        pulses.push(pulse);
                    }
                    TzxBlock::Pulses(pulses)
                }
                0x14 => {
                    let zero_pulse = reader.word()?;
                    let one_pulse = reader.word()?;
                    let used_bits = reader.byte()?;
                    let pause_ms = reader.word()?;
                    let length = reader.length24()?;
                    let data = reader.take(length)?.to_vec();
                    validate_data(&data, used_bits, zero_pulse, one_pulse)?;
                    TzxBlock::PureData(DataBlock {
                        pilot_pulse: 0,
                        pilot_count: 0,
                        sync1: 0,
                        sync2: 0,
                        zero_pulse,
                        one_pulse,
                        used_bits,
                        pause_ms,
                        data,
                    })
                }
                0x20 => TzxBlock::Pause(reader.word()?),
                0x2b => {
                    let length = reader.dword()?;
                    if length != 1 {
                        return Err(Error::InvalidInput(
                            "invalid TZX signal-level block length".into(),
                        ));
                    }
                    match reader.byte()? {
                        0 => TzxBlock::SetLevel(false),
                        1 => TzxBlock::SetLevel(true),
                        _ => return Err(Error::InvalidInput("invalid TZX signal level".into())),
                    }
                }
                0x21 | 0x30 => {
                    let length = usize::from(reader.byte()?);
                    TzxBlock::Metadata {
                        id,
                        data: reader.take(length)?.to_vec(),
                    }
                }
                0x22 => TzxBlock::Metadata {
                    id,
                    data: Vec::new(),
                },
                0x31 => {
                    let seconds = reader.byte()?;
                    let length = usize::from(reader.byte()?);
                    let mut data = vec![seconds];
                    data.extend_from_slice(reader.take(length)?);
                    TzxBlock::Metadata { id, data }
                }
                0x32 => {
                    let length = usize::from(reader.word()?);
                    TzxBlock::Metadata {
                        id,
                        data: reader.take(length)?.to_vec(),
                    }
                }
                0x33 => {
                    let count = usize::from(reader.byte()?);
                    TzxBlock::Metadata {
                        id,
                        data: reader.take(count * 3)?.to_vec(),
                    }
                }
                _ => return Err(Error::Unsupported("TZX block type")),
            };
            blocks.push(block);
        }
        Ok(Self {
            version: (major, minor),
            blocks,
            bytes: bytes.to_vec(),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn version(&self) -> (u8, u8) {
        self.version
    }

    pub fn blocks(&self) -> &[TzxBlock] {
        &self.blocks
    }
}

fn validate_data(data: &[u8], used_bits: u8, zero_pulse: u16, one_pulse: u16) -> Result<()> {
    if data.is_empty() || !(1..=8).contains(&used_bits) {
        return Err(Error::InvalidInput(
            "invalid TZX data length or used-bit count".into(),
        ));
    }
    if zero_pulse == 0 || one_pulse == 0 {
        return Err(Error::InvalidInput("zero TZX pulse length".into()));
    }
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        if length > self.bytes.len() - self.offset {
            return Err(Error::InvalidInput(format!(
                "truncated TZX at offset {}",
                self.offset
            )));
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn word(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn length24(&mut self) -> Result<usize> {
        let bytes = self.take(3)?;
        Ok(usize::from(bytes[0]) | usize::from(bytes[1]) << 8 | usize::from(bytes[2]) << 16)
    }

    fn dword(&mut self) -> Result<u32> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub struct TzxCodec;

impl MediaCodec for TzxCodec {
    type Output = TzxImage;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output> {
        TzxImage::parse(bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Stopped,
    Finished,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Start,
    Pilot(u16),
    Sync1,
    Sync2,
    Data { bit: usize, second: bool },
    Tone(u16),
    Pulses(usize),
    Pause(u16),
    PauseRest(u16),
    Next,
}

/// Streaming 3.5 MHz tape playback. No waveform is pre-expanded.
#[derive(Debug, Clone)]
pub struct TzxDeck {
    image: TzxImage,
    block: usize,
    phase: Phase,
    next_level: bool,
    level: bool,
    remaining: u64,
    status: PlaybackStatus,
}

impl TzxDeck {
    pub fn new(image: TzxImage) -> Self {
        let mut deck = Self {
            image,
            block: 0,
            phase: Phase::Start,
            next_level: false,
            level: false,
            remaining: 0,
            status: PlaybackStatus::Playing,
        };
        deck.prepare();
        deck
    }

    pub fn image(&self) -> &TzxImage {
        &self.image
    }

    pub fn status(&self) -> PlaybackStatus {
        self.status
    }

    /// Current EAR level; false is low.
    pub fn level(&self) -> bool {
        self.level
    }

    pub fn block_index(&self) -> usize {
        self.block
    }

    /// T-states to the next pulse or pause boundary, or zero when not playing.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Advance by up to `t_states`, returning the time consumed before a stop/end.
    pub fn advance(&mut self, mut t_states: u64) -> u64 {
        let mut consumed = 0;
        while t_states > 0 && self.status == PlaybackStatus::Playing {
            let step = t_states.min(self.remaining);
            self.remaining -= step;
            t_states -= step;
            consumed += step;
            if self.remaining == 0 {
                self.prepare();
            }
        }
        consumed
    }

    /// Resume after a stop block. Returns false if not stopped.
    pub fn resume(&mut self) -> bool {
        if self.status != PlaybackStatus::Stopped {
            return false;
        }
        self.status = PlaybackStatus::Playing;
        self.prepare();
        true
    }

    pub fn rewind(&mut self) {
        self.block = 0;
        self.phase = Phase::Start;
        self.next_level = false;
        self.level = false;
        self.remaining = 0;
        self.status = PlaybackStatus::Playing;
        self.prepare();
    }

    pub fn write_state(&self, writer: &mut StateWriter) -> Result<()> {
        writer.bytes(&self.image.bytes)?;
        writer.u64(self.block as u64);
        let (tag, value, flag) = match self.phase {
            Phase::Start => (0, 0, false),
            Phase::Pilot(n) => (1, u64::from(n), false),
            Phase::Sync1 => (2, 0, false),
            Phase::Sync2 => (3, 0, false),
            Phase::Data { bit, second } => (4, bit as u64, second),
            Phase::Tone(n) => (5, u64::from(n), false),
            Phase::Pulses(n) => (6, n as u64, false),
            Phase::Pause(n) => (7, u64::from(n), false),
            Phase::PauseRest(n) => (8, u64::from(n), false),
            Phase::Next => (9, 0, false),
        };
        writer.u8(tag);
        writer.u64(value);
        writer.u8(flag as u8);
        writer.u8(self.next_level as u8);
        writer.u8(self.level as u8);
        writer.u64(self.remaining);
        writer.u8(match self.status {
            PlaybackStatus::Playing => 0,
            PlaybackStatus::Stopped => 1,
            PlaybackStatus::Finished => 2,
        });
        Ok(())
    }

    pub fn read_state(reader: &mut StateReader<'_>) -> Result<Self> {
        let image = TzxImage::parse(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(format!("invalid TZX state: {error}")))?;
        let block = usize::try_from(reader.u64()?)
            .map_err(|_| Error::IncompatibleState("invalid TZX block index".into()))?;
        let tag = reader.u8()?;
        let value = reader.u64()?;
        let flag = reader.bool()?;
        let invalid = || Error::IncompatibleState("invalid TZX playback state".into());
        let count = || u16::try_from(value).map_err(|_| invalid());
        let phase = match (tag, value, flag) {
            (0, 0, false) => Phase::Start,
            (1, _, false) => Phase::Pilot(count()?),
            (2, 0, false) => Phase::Sync1,
            (3, 0, false) => Phase::Sync2,
            (4, _, second) => Phase::Data {
                bit: usize::try_from(value).map_err(|_| invalid())?,
                second,
            },
            (5, _, false) => Phase::Tone(count()?),
            (6, _, false) => Phase::Pulses(usize::try_from(value).map_err(|_| invalid())?),
            (7, _, false) => Phase::Pause(count()?),
            (8, _, false) => Phase::PauseRest(count()?),
            (9, 0, false) => Phase::Next,
            _ => return Err(invalid()),
        };
        let next_level = reader.bool()?;
        let level = reader.bool()?;
        let remaining = reader.u64()?;
        let status = match reader.u8()? {
            0 => PlaybackStatus::Playing,
            1 => PlaybackStatus::Stopped,
            2 => PlaybackStatus::Finished,
            _ => return Err(invalid()),
        };
        let deck = Self {
            image,
            block,
            phase,
            next_level,
            level,
            remaining,
            status,
        };
        if !deck.valid_state() {
            return Err(invalid());
        }
        Ok(deck)
    }

    fn valid_state(&self) -> bool {
        match self.status {
            PlaybackStatus::Stopped => {
                return self.remaining == 0
                    && self.level == self.next_level
                    && matches!(self.phase, Phase::Start)
                    && self.block > 0
                    && matches!(
                        self.image.blocks.get(self.block - 1),
                        Some(TzxBlock::Pause(0))
                    );
            }
            PlaybackStatus::Finished => {
                return self.remaining == 0
                    && self.level == self.next_level
                    && matches!(self.phase, Phase::Start)
                    && self.block == self.image.blocks.len();
            }
            PlaybackStatus::Playing => {}
        }
        let Some(block) = self.image.blocks.get(self.block) else {
            return false;
        };
        if self.remaining == 0 {
            return false;
        }
        let data = match block {
            TzxBlock::Standard(data) | TzxBlock::Turbo(data) | TzxBlock::PureData(data) => {
                Some(data)
            }
            _ => None,
        };
        let pause = match block {
            TzxBlock::Pause(ms) => *ms,
            _ => data.map_or(0, |data| data.pause_ms),
        };
        let pulse = match self.phase {
            Phase::Pilot(left) => data
                .filter(|data| !matches!(block, TzxBlock::PureData(_)) && left < data.pilot_count)
                .map(|data| data.pilot_pulse),
            Phase::Sync2 => data
                .filter(|_| !matches!(block, TzxBlock::PureData(_)))
                .map(|data| data.sync1),
            Phase::Data {
                bit: 0,
                second: false,
            } => data
                .filter(|_| !matches!(block, TzxBlock::PureData(_)))
                .map(|data| data.sync2),
            Phase::Data { bit, second } => data.and_then(|data| {
                let bits = (data.data.len() - 1) * 8 + usize::from(data.used_bits);
                let active = if second { bit } else { bit.checked_sub(1)? };
                if active >= bits {
                    return None;
                }
                let one = data.data[active / 8] & (0x80 >> (active % 8)) != 0;
                Some(if one { data.one_pulse } else { data.zero_pulse })
            }),
            Phase::Tone(left) => match block {
                TzxBlock::Tone { pulse, count } if left < *count => Some(*pulse),
                _ => None,
            },
            Phase::Pulses(index) => match block {
                TzxBlock::Pulses(pulses) if index > 0 => pulses.get(index - 1).copied(),
                _ => None,
            },
            Phase::PauseRest(left) => {
                return pause != 0
                    && u32::from(left) + 1 == u32::from(pause)
                    && !self.next_level
                    && self.remaining <= T_STATES_PER_MS;
            }
            Phase::Next => {
                return pause > 1
                    && !self.level
                    && !self.next_level
                    && self.remaining <= u64::from(pause - 1) * T_STATES_PER_MS;
            }
            _ => None,
        };
        pulse.is_some_and(|duration| {
            self.level != self.next_level && self.remaining <= u64::from(duration)
        })
    }

    fn pulse(&mut self, duration: u16) {
        self.level = self.next_level;
        self.next_level = !self.next_level;
        self.remaining = u64::from(duration);
    }

    fn prepare(&mut self) {
        while self.remaining == 0 && self.status == PlaybackStatus::Playing {
            let Some(block) = self.image.blocks.get(self.block) else {
                self.level = self.next_level;
                self.status = PlaybackStatus::Finished;
                return;
            };
            match self.phase {
                Phase::Start => match block {
                    TzxBlock::Standard(data) | TzxBlock::Turbo(data) => {
                        self.phase = Phase::Pilot(data.pilot_count);
                    }
                    TzxBlock::PureData(_) => {
                        self.phase = Phase::Data {
                            bit: 0,
                            second: false,
                        }
                    }
                    TzxBlock::Tone { count, .. } => self.phase = Phase::Tone(*count),
                    TzxBlock::Pulses(_) => self.phase = Phase::Pulses(0),
                    TzxBlock::Pause(0) => {
                        self.block += 1;
                        self.phase = Phase::Start;
                        self.level = self.next_level;
                        self.status = PlaybackStatus::Stopped;
                    }
                    TzxBlock::Pause(ms) => self.phase = Phase::Pause(*ms),
                    TzxBlock::SetLevel(level) => {
                        self.next_level = *level;
                        self.phase = Phase::Next;
                    }
                    TzxBlock::Metadata { .. } => self.phase = Phase::Next,
                },
                Phase::Pilot(left) => {
                    let data = data_block(block);
                    if left == 0 {
                        self.phase = Phase::Sync1;
                    } else {
                        let duration = data.pilot_pulse;
                        self.phase = Phase::Pilot(left - 1);
                        self.pulse(duration);
                    }
                }
                Phase::Sync1 => {
                    let duration = data_block(block).sync1;
                    self.phase = Phase::Sync2;
                    self.pulse(duration);
                }
                Phase::Sync2 => {
                    let duration = data_block(block).sync2;
                    self.phase = Phase::Data {
                        bit: 0,
                        second: false,
                    };
                    self.pulse(duration);
                }
                Phase::Data { bit, second } => {
                    let data = data_block(block);
                    let bits = (data.data.len() - 1) * 8 + usize::from(data.used_bits);
                    if bit == bits {
                        self.phase = Phase::Pause(data.pause_ms);
                    } else {
                        let one = data.data[bit / 8] & (0x80 >> (bit % 8)) != 0;
                        let duration = if one { data.one_pulse } else { data.zero_pulse };
                        self.phase = Phase::Data {
                            bit: bit + usize::from(second),
                            second: !second,
                        };
                        self.pulse(duration);
                    }
                }
                Phase::Tone(left) => {
                    if left == 0 {
                        self.phase = Phase::Next;
                    } else {
                        let TzxBlock::Tone { pulse, .. } = block else {
                            unreachable!("tone phase requires a tone block");
                        };
                        let duration = *pulse;
                        self.phase = Phase::Tone(left - 1);
                        self.pulse(duration);
                    }
                }
                Phase::Pulses(index) => {
                    if let TzxBlock::Pulses(pulses) = block {
                        if let Some(&duration) = pulses.get(index) {
                            self.phase = Phase::Pulses(index + 1);
                            self.pulse(duration);
                        } else {
                            self.phase = Phase::Next;
                        }
                    } else {
                        unreachable!("pulse phase requires a pulse sequence");
                    }
                }
                Phase::Pause(ms) => {
                    if ms == 0 {
                        self.phase = Phase::Next;
                    } else {
                        self.level = self.next_level;
                        self.next_level = false;
                        self.remaining = T_STATES_PER_MS;
                        self.phase = Phase::PauseRest(ms - 1);
                    }
                }
                Phase::PauseRest(ms) => {
                    self.level = false;
                    self.remaining = u64::from(ms) * T_STATES_PER_MS;
                    self.phase = Phase::Next;
                }
                Phase::Next => {
                    self.block += 1;
                    self.phase = Phase::Start;
                }
            }
        }
    }
}

fn data_block(block: &TzxBlock) -> &DataBlock {
    match block {
        TzxBlock::Standard(data) | TzxBlock::Turbo(data) | TzxBlock::PureData(data) => data,
        _ => unreachable!("data phase requires a data block"),
    }
}
