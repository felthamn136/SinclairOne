//! Original 16K Spectrum with instruction-timed, snapshot-rendered frames.

use std::time::Duration;

use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateReader, StateWriter},
    AudioFrame, Error, Frame, Machine, MachineInfo, Result, VideoFrame,
};
use sinclair_cpu_z80::{Bus, Z80};

use crate::{
    keyboard::KeyboardMatrix,
    memory::ROM_SIZE,
    video::{FrameConfig, ScreenMemory, Ula, FRAME_HEIGHT, FRAME_WIDTH, SCREEN_BYTES},
};

pub const INFO: MachineInfo = MachineInfo {
    id: "spectrum16",
    name: "ZX Spectrum 16K",
};
pub const RAM_SIZE: usize = 16 * 1024;
pub const CLOCK_HZ: u32 = 3_500_000;
pub const FRAME_TSTATES: u32 = 69_888;
const SAMPLE_RATE: u32 = 44_100;
const STATE_VERSION: u32 = 1;

pub struct Memory {
    rom: Box<[u8]>,
    ram: Box<[u8]>,
}

impl Memory {
    pub fn new(rom: &[u8]) -> Result<Self> {
        if rom.len() != ROM_SIZE {
            return Err(Error::InvalidInput(format!(
                "Spectrum 16K ROM must be {ROM_SIZE} bytes, got {}",
                rom.len()
            )));
        }
        Ok(Self {
            rom: rom.into(),
            ram: vec![0; RAM_SIZE].into_boxed_slice(),
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x3fff => self.rom[usize::from(address)],
            0x4000..=0x7fff => self.ram[usize::from(address - 0x4000)],
            _ => 0xff,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        if (0x4000..=0x7fff).contains(&address) {
            self.ram[usize::from(address - 0x4000)] = value;
        }
    }

    pub fn ram(&self) -> &[u8] {
        &self.ram
    }
}

impl ScreenMemory for Memory {
    fn screen_ram(&self) -> &[u8] {
        &self.ram[..SCREEN_BYTES]
    }
}

struct Spectrum16Bus<'a> {
    memory: &'a mut Memory,
    keyboard: &'a KeyboardMatrix,
    border: &'a mut u8,
    speaker: &'a mut bool,
}

impl Bus for Spectrum16Bus<'_> {
    fn read_memory(&mut self, address: u16) -> u8 {
        self.memory.read(address)
    }

    fn write_memory(&mut self, address: u16, value: u8) {
        self.memory.write(address, value);
    }

    fn read_port(&mut self, port: u16) -> u8 {
        if port & 1 == 0 {
            0xe0 | self.keyboard.column_mask(port)
        } else {
            0xff
        }
    }

    fn write_port(&mut self, port: u16, value: u8) {
        if port & 1 == 0 {
            *self.border = value & 7;
            *self.speaker = value & 0x10 != 0;
        }
    }
}

pub struct Spectrum16 {
    cpu: Z80,
    memory: Memory,
    keyboard: KeyboardMatrix,
    border: u8,
    speaker: bool,
    phase: u32,
    sample_phase: u32,
    frame_number: u64,
    video: Vec<u8>,
    audio: Vec<f32>,
}

impl Spectrum16 {
    pub fn new(rom: &[u8]) -> Result<Self> {
        Ok(Self {
            cpu: Z80::new(),
            memory: Memory::new(rom)?,
            keyboard: KeyboardMatrix::new(),
            border: 0,
            speaker: false,
            phase: 0,
            sample_phase: 0,
            frame_number: 0,
            video: Vec::new(),
            audio: Vec::new(),
        })
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }
}

impl Machine for Spectrum16 {
    fn info(&self) -> MachineInfo {
        INFO
    }

    fn frame_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            f64::from(FRAME_TSTATES) / f64::from(CLOCK_HZ),
        ))
    }

    fn reset(&mut self) -> Result<()> {
        self.cpu.reset();
        self.cpu.set_irq(false);
        self.border = 0;
        self.speaker = false;
        self.phase = 0;
        self.sample_phase = 0;
        self.frame_number = 0;
        Ok(())
    }

    fn input(&mut self, event: InputEvent) -> Result<()> {
        self.keyboard.apply_input(&event)
    }

    fn run_frame(&mut self) -> Result<Frame<'_>> {
        self.audio.clear();
        while self.phase < FRAME_TSTATES {
            self.cpu.set_irq(self.phase < 32);
            let speaker = self.speaker;
            let cycles = self.cpu.step(&mut Spectrum16Bus {
                memory: &mut self.memory,
                keyboard: &self.keyboard,
                border: &mut self.border,
                speaker: &mut self.speaker,
            });
            self.phase = self
                .phase
                .checked_add(cycles)
                .ok_or_else(|| Error::InvalidInput("Spectrum 16K frame timing overflow".into()))?;
            let mut sample_time =
                u64::from(self.sample_phase) + u64::from(cycles) * u64::from(SAMPLE_RATE);
            while sample_time >= u64::from(CLOCK_HZ) {
                let sample = if speaker { 0.25 } else { -0.25 };
                self.audio.extend_from_slice(&[sample, sample]);
                sample_time -= u64::from(CLOCK_HZ);
            }
            self.sample_phase = sample_time as u32;
        }
        self.phase -= FRAME_TSTATES;
        self.video = Ula::new().render_frame(
            &self.memory,
            &FrameConfig {
                frame_number: self.frame_number,
                border_colour: self.border,
                border_changes: &[],
            },
        )?;
        self.frame_number = self.frame_number.wrapping_add(1);
        Ok(Frame {
            video: VideoFrame {
                width: FRAME_WIDTH as u32,
                height: FRAME_HEIGHT as u32,
                rgba: &self.video,
            },
            audio: AudioFrame {
                sample_rate: SAMPLE_RATE,
                samples: &self.audio,
            },
        })
    }

    fn load_media(&mut self, _: &Media<'_>) -> Result<()> {
        Err(Error::Unsupported("Spectrum 16K media loading"))
    }

    fn save_state(&self) -> Result<State> {
        let mut writer = StateWriter::new();
        writer.bytes(&self.memory.rom)?;
        writer.bytes(&self.cpu.state_bytes())?;
        writer.bytes(&self.memory.ram)?;
        writer.bytes(&self.keyboard.state_rows())?;
        writer.u8(self.border);
        writer.u8(u8::from(self.speaker));
        writer.u32(self.phase);
        writer.u32(self.sample_phase);
        writer.u64(self.frame_number);
        Ok(State::sealed(INFO.id, STATE_VERSION, writer.finish()))
    }

    fn load_state(&mut self, state: &State) -> Result<()> {
        let mut reader = StateReader::new(state.checked_payload(INFO.id, STATE_VERSION)?);
        if reader.bytes()? != self.memory.rom.as_ref() {
            return Err(Error::IncompatibleState("Spectrum 16K ROM mismatch".into()));
        }
        let cpu = Z80::from_state_bytes(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.into()))?;
        let ram = reader.bytes()?;
        if ram.len() != RAM_SIZE {
            return Err(Error::IncompatibleState("invalid Spectrum 16K RAM".into()));
        }
        let rows = reader
            .bytes()?
            .try_into()
            .map_err(|_| Error::IncompatibleState("invalid keyboard matrix size".into()))?;
        let keyboard = KeyboardMatrix::from_state_rows(rows)?;
        let border = reader.u8()?;
        if border > 7 {
            return Err(Error::IncompatibleState(
                "invalid Spectrum border colour".into(),
            ));
        }
        let speaker = reader.bool()?;
        let phase = reader.u32()?;
        let sample_phase = reader.u32()?;
        if sample_phase >= CLOCK_HZ {
            return Err(Error::IncompatibleState("invalid audio phase".into()));
        }
        let frame_number = reader.u64()?;
        reader.finish()?;
        self.memory.ram.copy_from_slice(ram);
        self.cpu = cpu;
        self.keyboard = keyboard;
        self.border = border;
        self.speaker = speaker;
        self.phase = phase;
        self.sample_phase = sample_phase;
        self.frame_number = frame_number;
        Ok(())
    }
}
