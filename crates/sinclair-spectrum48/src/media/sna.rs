//! Codec for the standard 48K SNA snapshot format.

use sinclair_core::{media::MediaCodec, Error, Result};
use sinclair_cpu_z80::{InterruptMode, Z80};

use crate::memory::{Memory, RAM_SIZE, ROM_SIZE};

pub const HEADER_SIZE: usize = 27;
pub const SNA_SIZE: usize = HEADER_SIZE + RAM_SIZE;

/// A resumed CPU, the snapshot RAM, and the ULA border colour.
#[derive(Debug, Clone)]
pub struct SnaSnapshot {
    cpu: Z80,
    ram: Box<[u8]>,
    border_colour: u8,
}

impl SnaSnapshot {
    /// Parse a 48K SNA, emulating RETN to recover PC and interrupt state.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != SNA_SIZE {
            return Err(Error::InvalidInput(format!(
                "48K SNA must be {SNA_SIZE} bytes, got {}",
                bytes.len()
            )));
        }
        let word = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let sp = word(23);
        let stack = stack_offset(sp)?;
        let im = match bytes[25] {
            0 => InterruptMode::Mode0,
            1 => InterruptMode::Mode1,
            2 => InterruptMode::Mode2,
            value => {
                return Err(Error::InvalidInput(format!(
                    "invalid SNA interrupt mode {value}"
                )))
            }
        };
        if bytes[26] > 7 {
            return Err(Error::InvalidInput(format!(
                "invalid SNA border colour {}",
                bytes[26]
            )));
        }
        let ram = bytes[HEADER_SIZE..].to_vec().into_boxed_slice();
        let mut cpu = Z80::new();
        cpu.i = bytes[0];
        cpu.set_hl_alt(word(1));
        cpu.set_de_alt(word(3));
        cpu.set_bc_alt(word(5));
        cpu.set_af_alt(word(7));
        cpu.set_hl(word(9));
        cpu.set_de(word(11));
        cpu.set_bc(word(13));
        cpu.iy = word(15);
        cpu.ix = word(17);
        cpu.iff2 = bytes[19] & 4 != 0;
        cpu.iff1 = cpu.iff2;
        cpu.r = bytes[20];
        cpu.set_af(word(21));
        cpu.sp = sp.wrapping_add(2);
        cpu.pc = u16::from_le_bytes([ram[stack], ram[stack + 1]]);
        cpu.im = im;
        Ok(Self {
            cpu,
            ram,
            border_colour: bytes[26],
        })
    }

    /// Capture a machine state that can be represented by 48K SNA.
    pub fn capture(cpu: &Z80, memory: &Memory, border_colour: u8) -> Result<Self> {
        let snapshot = Self {
            cpu: cpu.clone(),
            ram: memory.ram().into(),
            border_colour,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    pub fn border_colour(&self) -> u8 {
        self.border_colour
    }

    /// Serialize, pushing PC into a copy of RAM, never the source memory.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let cpu = &self.cpu;
        let sp = cpu.sp.wrapping_sub(2);
        let stack = stack_offset(sp)?;
        let mut bytes = vec![0; SNA_SIZE];
        bytes[0] = cpu.i;
        for (offset, value) in [
            (1, cpu.hl_alt()),
            (3, cpu.de_alt()),
            (5, cpu.bc_alt()),
            (7, cpu.af_alt()),
            (9, cpu.hl()),
            (11, cpu.de()),
            (13, cpu.bc()),
            (15, cpu.iy),
            (17, cpu.ix),
            (21, cpu.af()),
            (23, sp),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[19] = if cpu.iff2 { 4 } else { 0 };
        bytes[20] = cpu.r;
        bytes[25] = match cpu.im {
            InterruptMode::Mode0 => 0,
            InterruptMode::Mode1 => 1,
            InterruptMode::Mode2 => 2,
        };
        bytes[26] = self.border_colour;
        bytes[HEADER_SIZE..].copy_from_slice(&self.ram);
        bytes[HEADER_SIZE + stack..HEADER_SIZE + stack + 2].copy_from_slice(&cpu.pc.to_le_bytes());
        Ok(bytes)
    }

    fn validate(&self) -> Result<()> {
        stack_offset(self.cpu.sp.wrapping_sub(2))?;
        if self.border_colour > 7 {
            return Err(Error::InvalidInput(format!(
                "invalid SNA border colour {}",
                self.border_colour
            )));
        }
        if self.cpu.iff1 != self.cpu.iff2
            || self.cpu.halted
            || self.cpu.has_pending_interrupt_state()
        {
            return Err(Error::InvalidInput(
                "CPU interrupt or HALT state cannot be represented in 48K SNA".into(),
            ));
        }
        Ok(())
    }
}

fn stack_offset(sp: u16) -> Result<usize> {
    let address = usize::from(sp);
    if !(ROM_SIZE..ROM_SIZE + RAM_SIZE - 1).contains(&address) {
        return Err(Error::InvalidInput(format!(
            "SNA stack pointer {sp:04x} does not point to two RAM bytes"
        )));
    }
    Ok(address - ROM_SIZE)
}

pub struct SnaCodec;

impl MediaCodec for SnaCodec {
    type Output = SnaSnapshot;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output> {
        SnaSnapshot::parse(bytes)
    }
}
