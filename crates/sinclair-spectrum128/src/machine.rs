use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateReader, StateWriter},
    Error, Frame, Machine, MachineInfo, Result,
};
use sinclair_cpu_z80::Z80;
use sinclair_spectrum48::{
    keyboard::KeyboardMatrix,
    media::tzx::{TzxDeck, TzxImage},
};

use crate::memory::Memory;

pub const INFO: MachineInfo = MachineInfo {
    id: "spectrum128",
    name: "ZX Spectrum 128K",
};
const STATE_VERSION: u32 = 1;

pub struct Spectrum128 {
    cpu: Z80,
    memory: Memory,
    keyboard: KeyboardMatrix,
    tzx_tape: Option<TzxDeck>,
}

impl Spectrum128 {
    pub fn new(rom0: &[u8], rom1: &[u8]) -> Result<Self> {
        Ok(Self {
            cpu: Z80::new(),
            memory: Memory::new(rom0, rom1)?,
            keyboard: KeyboardMatrix::new(),
            tzx_tape: None,
        })
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

    pub fn tzx_tape(&self) -> Option<&TzxDeck> {
        self.tzx_tape.as_ref()
    }

    pub fn tzx_tape_mut(&mut self) -> Option<&mut TzxDeck> {
        self.tzx_tape.as_mut()
    }
}

impl Machine for Spectrum128 {
    fn info(&self) -> MachineInfo {
        INFO
    }

    fn reset(&mut self) -> Result<()> {
        self.cpu.reset();
        self.memory.reset_paging();
        Ok(())
    }

    fn input(&mut self, event: InputEvent) -> Result<()> {
        self.keyboard.apply_input(&event)
    }

    fn run_frame(&mut self) -> Result<Frame<'_>> {
        Err(Error::Unsupported("Spectrum 128K frame execution"))
    }

    fn load_media(&mut self, media: &Media<'_>) -> Result<()> {
        if media.format != "tzx" {
            return Err(Error::Unsupported("Spectrum 128K media format"));
        }
        let image = TzxImage::parse(media.bytes)?;
        self.tzx_tape = Some(TzxDeck::new(image));
        Ok(())
    }

    fn save_state(&self) -> Result<State> {
        let mut writer = StateWriter::new();
        writer.bytes(&self.cpu.state_bytes())?;
        self.memory.write_state(&mut writer)?;
        writer.bytes(&self.keyboard.state_rows())?;
        if let Some(tape) = &self.tzx_tape {
            writer.u8(1);
            tape.write_state(&mut writer)?;
        } else {
            writer.u8(0);
        }
        Ok(State::sealed(INFO.id, STATE_VERSION, writer.finish()))
    }

    fn load_state(&mut self, state: &State) -> Result<()> {
        let mut reader = StateReader::new(state.checked_payload(INFO.id, STATE_VERSION)?);
        let cpu = Z80::from_state_bytes(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.into()))?;
        let memory = self.memory.read_state(&mut reader)?;
        let rows: [u8; 8] = reader
            .bytes()?
            .try_into()
            .map_err(|_| Error::IncompatibleState("invalid keyboard matrix size".into()))?;
        let keyboard = KeyboardMatrix::from_state_rows(rows)?;
        let tzx_tape = match reader.u8()? {
            0 => None,
            1 => Some(TzxDeck::read_state(&mut reader)?),
            _ => {
                return Err(Error::IncompatibleState(
                    "invalid Spectrum tape state".into(),
                ))
            }
        };
        reader.finish()?;
        self.cpu = cpu;
        self.memory = memory;
        self.keyboard = keyboard;
        self.tzx_tape = tzx_tape;
        Ok(())
    }
}
