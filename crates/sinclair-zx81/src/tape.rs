use sinclair_core::{
    state::{StateReader, StateWriter},
    Error, Result,
};

use crate::program::Program;

pub const CPU_HZ: u64 = 3_250_000;
const LEADER: u64 = CPU_HZ * 5;
const HIGH: u64 = 503;
const LOW: u64 = 472;
const GAP: u64 = 4_861;

pub enum Tape {
    Wav(WavTape),
    Program(ProgramTape),
}

impl Tape {
    pub fn level(&self) -> bool {
        match self {
            Self::Wav(tape) => tape.level(),
            Self::Program(tape) => tape.level(),
        }
    }

    pub fn advance(&mut self, tstates: u64) -> u64 {
        match self {
            Self::Wav(tape) => tape.advance(tstates),
            Self::Program(tape) => tape.advance(tstates),
        }
    }

    pub fn finished(&self) -> bool {
        match self {
            Self::Wav(tape) => tape.finished(),
            Self::Program(tape) => tape.finished(),
        }
    }

    pub fn write_state(&self, writer: &mut StateWriter) -> Result<()> {
        match self {
            Self::Wav(tape) => {
                writer.u8(1);
                tape.write_state(writer)
            }
            Self::Program(tape) => {
                writer.u8(2);
                tape.write_state(writer)
            }
        }
    }

    pub fn read_state(reader: &mut StateReader<'_>) -> Result<Self> {
        match reader.u8()? {
            1 => Ok(Self::Wav(WavTape::read_state(reader)?)),
            2 => Ok(Self::Program(ProgramTape::read_state(reader)?)),
            _ => Err(Error::IncompatibleState("invalid ZX81 tape type".into())),
        }
    }
}

/// A headerless .p image turned into an unnamed ZX81 cassette recording.
pub struct ProgramTape {
    program: Program,
    leader: bool,
    bit: usize,
    pulse: u8,
    remaining: u64,
}

impl ProgramTape {
    pub fn new(program: Program) -> Self {
        Self {
            program,
            leader: true,
            bit: 0,
            pulse: 0,
            remaining: LEADER,
        }
    }

    fn bit_count(&self) -> usize {
        (self.program.bytes().len() + 1) * 8
    }

    fn wave_count(&self) -> u8 {
        let byte = if self.bit < 8 {
            0x80
        } else {
            self.program.bytes()[self.bit / 8 - 1]
        };
        if byte & (0x80 >> (self.bit % 8)) == 0 {
            4
        } else {
            9
        }
    }

    fn pulse_length(&self) -> u64 {
        if self.pulse % 2 == 0 {
            HIGH
        } else if self.pulse + 1 == self.wave_count() * 2 {
            GAP
        } else {
            LOW
        }
    }

    pub fn level(&self) -> bool {
        !self.leader && !self.finished() && self.pulse % 2 == 0
    }

    pub fn finished(&self) -> bool {
        !self.leader && self.bit == self.bit_count()
    }

    pub fn advance(&mut self, tstates: u64) -> u64 {
        let mut left = tstates;
        while left > 0 && !self.finished() {
            let step = left.min(self.remaining);
            self.remaining -= step;
            left -= step;
            if self.remaining == 0 {
                if self.leader {
                    self.leader = false;
                } else {
                    self.pulse += 1;
                    if self.pulse == self.wave_count() * 2 {
                        self.pulse = 0;
                        self.bit += 1;
                    }
                }
                if !self.finished() {
                    self.remaining = self.pulse_length();
                }
            }
        }
        tstates - left
    }

    pub fn write_state(&self, writer: &mut StateWriter) -> Result<()> {
        writer.bytes(self.program.bytes())?;
        writer.u8(u8::from(self.leader));
        writer.u64(self.bit as u64);
        writer.u8(self.pulse);
        writer.u64(self.remaining);
        Ok(())
    }

    pub fn read_state(reader: &mut StateReader<'_>) -> Result<Self> {
        let program = Program::parse(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.to_string()))?;
        let leader = reader.bool()?;
        let bit = usize::try_from(reader.u64()?)
            .map_err(|_| Error::IncompatibleState("invalid tape bit position".into()))?;
        let pulse = reader.u8()?;
        let remaining = reader.u64()?;
        let tape = Self {
            program,
            leader,
            bit,
            pulse,
            remaining,
        };
        let valid = if tape.leader {
            tape.bit == 0 && tape.pulse == 0 && (1..=LEADER).contains(&tape.remaining)
        } else if tape.bit == tape.bit_count() {
            tape.pulse == 0 && tape.remaining == 0
        } else if tape.bit < tape.bit_count() && tape.pulse < tape.wave_count() * 2 {
            (1..=tape.pulse_length()).contains(&tape.remaining)
        } else {
            false
        };
        if !valid {
            return Err(Error::IncompatibleState(
                "invalid program tape position".into(),
            ));
        }
        Ok(tape)
    }
}

/// PCM WAV recording presented as the ZX81 EAR input.
pub struct WavTape {
    image: Vec<u8>,
    samples: Vec<i16>,
    sample_rate: u32,
    position: usize,
    phase: u64,
}

fn word(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn dword(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

impl WavTape {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(Error::InvalidInput("invalid WAV header".into()));
        }
        let declared = usize::try_from(dword(&bytes[4..8]))
            .map_err(|_| Error::InvalidInput("WAV length is too large".into()))?;
        if declared.checked_add(8) != Some(bytes.len()) {
            return Err(Error::InvalidInput("invalid WAV length".into()));
        }
        let mut offset = 12;
        let mut format = None;
        let mut data = None;
        while offset < bytes.len() {
            if bytes.len() - offset < 8 {
                return Err(Error::InvalidInput("truncated WAV chunk".into()));
            }
            let id = &bytes[offset..offset + 4];
            let length = usize::try_from(dword(&bytes[offset + 4..offset + 8]))
                .map_err(|_| Error::InvalidInput("WAV chunk is too large".into()))?;
            let start = offset + 8;
            let end = start
                .checked_add(length)
                .filter(|&end| end <= bytes.len())
                .ok_or_else(|| Error::InvalidInput("truncated WAV chunk".into()))?;
            let next = end
                .checked_add(length % 2)
                .filter(|&next| next <= bytes.len())
                .ok_or_else(|| Error::InvalidInput("missing WAV chunk padding".into()))?;
            match id {
                b"fmt " if format.is_none() => format = Some(&bytes[start..end]),
                b"data" if data.is_none() => data = Some(&bytes[start..end]),
                b"fmt " | b"data" => return Err(Error::InvalidInput("duplicate WAV chunk".into())),
                _ => {}
            }
            offset = next;
        }
        let format = format.ok_or_else(|| Error::InvalidInput("missing WAV format".into()))?;
        let data = data.ok_or_else(|| Error::InvalidInput("missing WAV data".into()))?;
        if format.len() < 16 {
            return Err(Error::InvalidInput("truncated WAV format".into()));
        }
        let channels = word(&format[2..4]);
        let sample_rate = dword(&format[4..8]);
        let block_align = word(&format[12..14]);
        let bits = word(&format[14..16]);
        if word(format) != 1 || !(channels == 1 || channels == 2) || !(bits == 8 || bits == 16) {
            return Err(Error::Unsupported(
                "WAV format: use 8/16-bit PCM mono or stereo",
            ));
        }
        if sample_rate == 0
            || block_align != channels * (bits / 8)
            || sample_rate.checked_mul(u32::from(block_align)) != Some(dword(&format[8..12]))
            || data.is_empty()
            || data.len() % usize::from(block_align) != 0
        {
            return Err(Error::InvalidInput("invalid WAV PCM data".into()));
        }
        let bytes_per_sample = usize::from(bits / 8);
        let mut samples = Vec::with_capacity(data.len() / usize::from(block_align));
        for frame in data.chunks_exact(usize::from(block_align)) {
            let mut sum = 0i32;
            for sample in frame.chunks_exact(bytes_per_sample) {
                sum += if bits == 8 {
                    (i32::from(sample[0]) - 128) * 256
                } else {
                    i32::from(i16::from_le_bytes([sample[0], sample[1]]))
                };
            }
            samples.push((sum / i32::from(channels)) as i16);
        }
        Ok(Self {
            image: bytes.to_vec(),
            samples,
            sample_rate,
            position: 0,
            phase: 0,
        })
    }

    pub fn level(&self) -> bool {
        self.samples
            .get(self.position)
            .is_some_and(|&sample| sample > 0)
    }

    pub fn finished(&self) -> bool {
        self.position == self.samples.len()
    }

    /// Advance in ZX81 CPU T-states, retaining fractional sample timing.
    pub fn advance(&mut self, tstates: u64) -> u64 {
        if self.finished() {
            return 0;
        }
        let remaining = (self.samples.len() - self.position) as u128 * u128::from(CPU_HZ)
            - u128::from(self.phase);
        let rate = u128::from(self.sample_rate);
        let until_end = (remaining + rate - 1) / rate;
        let consumed = u128::from(tstates).min(until_end) as u64;
        let elapsed = u128::from(self.phase) + u128::from(consumed) * rate;
        if u128::from(consumed) == until_end {
            self.position = self.samples.len();
            self.phase = 0;
        } else {
            self.position += (elapsed / u128::from(CPU_HZ)) as usize;
            self.phase = (elapsed % u128::from(CPU_HZ)) as u64;
        }
        consumed
    }

    pub fn write_state(&self, writer: &mut StateWriter) -> Result<()> {
        writer.bytes(&self.image)?;
        writer.u64(self.position as u64);
        writer.u64(self.phase);
        Ok(())
    }

    pub fn read_state(reader: &mut StateReader<'_>) -> Result<Self> {
        let mut tape = Self::parse(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.to_string()))?;
        let position = usize::try_from(reader.u64()?)
            .map_err(|_| Error::IncompatibleState("invalid WAV position".into()))?;
        let phase = reader.u64()?;
        if position > tape.samples.len()
            || phase >= CPU_HZ
            || (position == tape.samples.len() && phase != 0)
        {
            return Err(Error::IncompatibleState(
                "invalid WAV playback position".into(),
            ));
        }
        tape.position = position;
        tape.phase = phase;
        Ok(tape)
    }
}
