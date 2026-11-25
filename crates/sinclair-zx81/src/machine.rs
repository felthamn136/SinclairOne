use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateReader, StateWriter},
    Error, Frame, Machine, MachineInfo, Result,
};
use sinclair_cpu_z80::Z80;
use sinclair_spectrum48::keyboard::{Key, KeyboardMatrix};

use crate::{
    bus::Zx81Bus,
    memory::{Memory, RamSize},
    program::Program,
    tape::{ProgramTape, Tape, WavTape},
};

pub const INFO: MachineInfo = MachineInfo {
    id: "zx81",
    name: "Sinclair ZX81",
};
const STATE_VERSION: u32 = 3;
const TSTATES_PER_LINE: u32 = 207;

/// ZX81 machine boundary with instruction-level, not cycle-accurate, stepping.
pub struct Zx81 {
    cpu: Z80,
    memory: Memory,
    keyboard: KeyboardMatrix,
    tape: Option<Tape>,
    nmi_enabled: bool,
    line_phase: u32,
}

impl Zx81 {
    pub fn new(rom: &[u8], ram_size: RamSize) -> Result<Self> {
        Ok(Self {
            cpu: Z80::new(),
            memory: Memory::new(rom, ram_size)?,
            keyboard: KeyboardMatrix::new(),
            tape: None,
            nmi_enabled: false,
            line_phase: 0,
        })
    }

    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }

    pub fn keyboard(&self) -> &KeyboardMatrix {
        &self.keyboard
    }

    pub fn tape(&self) -> Option<&Tape> {
        self.tape.as_ref()
    }

    pub fn tape_mut(&mut self) -> Option<&mut Tape> {
        self.tape.as_mut()
    }

    /// Execute one instruction, advancing the tape and line-timer approximation.
    pub fn step_instruction(&mut self) -> u32 {
        self.cpu.set_irq(self.cpu.r & 0x40 == 0);
        let mut bus = Zx81Bus {
            memory: &mut self.memory,
            keyboard: &self.keyboard,
            tape: &self.tape,
            nmi_enabled: &mut self.nmi_enabled,
            interrupt_acknowledged: false,
        };
        let cycles = self.cpu.step(&mut bus);
        if bus.interrupt_acknowledged {
            self.line_phase = 0;
        }
        let elapsed = self.line_phase + cycles;
        self.line_phase = elapsed % TSTATES_PER_LINE;
        if elapsed >= TSTATES_PER_LINE && self.nmi_enabled {
            self.cpu.request_nmi();
        }
        if let Some(tape) = &mut self.tape {
            tape.advance(u64::from(cycles));
        }
        cycles
    }
}

impl Machine for Zx81 {
    fn info(&self) -> MachineInfo {
        INFO
    }

    fn reset(&mut self) -> Result<()> {
        self.cpu.reset();
        self.nmi_enabled = false;
        self.line_phase = 0;
        Ok(())
    }

    fn input(&mut self, event: InputEvent) -> Result<()> {
        let key = match event.control.as_str() {
            "shift" | "caps_shift" => Key::CapsShift,
            "." => Key::SymbolShift,
            "symbol_shift" => return Err(Error::InvalidInput("unknown ZX81 key".into())),
            _ => Key::from_control_id(&event.control)?,
        };
        self.keyboard.set_key(key, event.pressed);
        Ok(())
    }

    fn run_frame(&mut self) -> Result<Frame<'_>> {
        Err(Error::Unsupported("ZX81 frame execution"))
    }

    fn load_media(&mut self, media: &Media<'_>) -> Result<()> {
        match media.format {
            "p" => Program::parse(media.bytes)?.copy_to_memory(&mut self.memory),
            "p-tape" => {
                self.tape = Some(Tape::Program(ProgramTape::new(Program::parse(
                    media.bytes,
                )?)));
                Ok(())
            }
            "wav" => {
                self.tape = Some(Tape::Wav(WavTape::parse(media.bytes)?));
                Ok(())
            }
            _ => Err(Error::Unsupported("ZX81 media format")),
        }
    }

    fn save_state(&self) -> Result<State> {
        let mut writer = StateWriter::new();
        writer.bytes(self.memory.rom())?;
        writer.bytes(&self.cpu.state_bytes())?;
        writer.bytes(self.memory.ram())?;
        writer.bytes(&self.keyboard.state_rows())?;
        writer.u8(u8::from(self.nmi_enabled));
        writer.u32(self.line_phase);
        if let Some(tape) = &self.tape {
            writer.u8(1);
            tape.write_state(&mut writer)?;
        } else {
            writer.u8(0);
        }
        Ok(State::sealed(INFO.id, STATE_VERSION, writer.finish()))
    }

    fn load_state(&mut self, state: &State) -> Result<()> {
        let mut reader = StateReader::new(state.checked_payload(INFO.id, STATE_VERSION)?);
        if reader.bytes()? != self.memory.rom() {
            return Err(Error::IncompatibleState("ZX81 ROM mismatch".into()));
        }
        let cpu = Z80::from_state_bytes(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.into()))?;
        let ram = reader.bytes()?;
        if ram.len() != self.memory.ram().len() {
            return Err(Error::IncompatibleState("ZX81 RAM size mismatch".into()));
        }
        let rows = reader
            .bytes()?
            .try_into()
            .map_err(|_| Error::IncompatibleState("invalid keyboard matrix size".into()))?;
        let keyboard = KeyboardMatrix::from_state_rows(rows)?;
        let nmi_enabled = reader.bool()?;
        let line_phase = reader.u32()?;
        if line_phase >= TSTATES_PER_LINE {
            return Err(Error::IncompatibleState("invalid ZX81 line phase".into()));
        }
        let tape = match reader.u8()? {
            0 => None,
            1 => Some(Tape::read_state(&mut reader)?),
            _ => return Err(Error::IncompatibleState("invalid ZX81 tape state".into())),
        };
        reader.finish()?;
        self.memory.load_ram(ram)?;
        self.cpu = cpu;
        self.keyboard = keyboard;
        self.nmi_enabled = nmi_enabled;
        self.line_phase = line_phase;
        self.tape = tape;
        Ok(())
    }
}
