use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateReader, StateWriter},
    Error, Frame, Machine, MachineInfo, Result,
};
use sinclair_cpu_z80::Z80;

use crate::{
    keyboard::KeyboardMatrix,
    media::{
        sna::SnaSnapshot,
        tap::{TapDeck, TapImage},
        tzx::{TzxDeck, TzxImage},
    },
    memory::Memory,
};

pub const INFO: MachineInfo = MachineInfo {
    id: "spectrum48",
    name: "ZX Spectrum 48K",
};
const STATE_VERSION: u32 = 1;

pub struct Spectrum48 {
    // Components are retained here until machine scheduling and I/O are implemented.
    cpu: Z80,
    memory: Memory,
    border_colour: u8,
    keyboard: KeyboardMatrix,
    tape: Option<TapDeck>,
    tzx_tape: Option<TzxDeck>,
}

impl Spectrum48 {
    pub fn new(rom: &[u8]) -> Result<Self> {
        Ok(Self {
            cpu: Z80::new(),
            memory: Memory::new(rom)?,
            border_colour: 0,
            keyboard: KeyboardMatrix::new(),
            tape: None,
            tzx_tape: None,
        })
    }

    pub fn keyboard(&self) -> &KeyboardMatrix {
        &self.keyboard
    }

    pub fn tape(&self) -> Option<&TapDeck> {
        self.tape.as_ref()
    }

    pub fn tzx_tape(&self) -> Option<&TzxDeck> {
        self.tzx_tape.as_ref()
    }

    pub fn tzx_tape_mut(&mut self) -> Option<&mut TzxDeck> {
        self.tzx_tape.as_mut()
    }

    pub fn cpu(&self) -> &Z80 {
        &self.cpu
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn border_colour(&self) -> u8 {
        self.border_colour
    }

    /// Load a 48K SNA without modifying the machine on validation failure.
    pub fn load_sna(&mut self, bytes: &[u8]) -> Result<()> {
        let snapshot = SnaSnapshot::parse(bytes)?;
        self.memory.load_ram(snapshot.ram())?;
        self.cpu = snapshot.cpu().clone();
        self.border_colour = snapshot.border_colour();
        Ok(())
    }

    /// Create a 48K SNA. PC is written to a copy of the RAM stack.
    pub fn save_sna(&self) -> Result<Vec<u8>> {
        SnaSnapshot::capture(&self.cpu, &self.memory, self.border_colour)?.to_bytes()
    }
}

impl Machine for Spectrum48 {
    fn info(&self) -> MachineInfo {
        INFO
    }

    fn reset(&mut self) -> Result<()> {
        Err(Error::Unsupported("Spectrum 48K reset"))
    }

    fn input(&mut self, event: InputEvent) -> Result<()> {
        self.keyboard.apply_input(&event)
    }

    fn run_frame(&mut self) -> Result<Frame<'_>> {
        Err(Error::Unsupported("Spectrum 48K frame execution"))
    }

    fn load_media(&mut self, media: &Media<'_>) -> Result<()> {
        match media.format {
            "tap" => {
                let image = TapImage::parse(media.bytes)?;
                self.tape = Some(TapDeck::new(image));
                self.tzx_tape = None;
                Ok(())
            }
            "tzx" => {
                let image = TzxImage::parse(media.bytes)?;
                self.tzx_tape = Some(TzxDeck::new(image));
                self.tape = None;
                Ok(())
            }
            "sna" => self.load_sna(media.bytes),
            _ => Err(Error::Unsupported("Spectrum 48K media format")),
        }
    }

    fn save_state(&self) -> Result<State> {
        let mut writer = StateWriter::new();
        writer.bytes(self.memory.rom())?;
        writer.bytes(&self.cpu.state_bytes())?;
        writer.bytes(self.memory.ram())?;
        writer.u8(self.border_colour);
        writer.bytes(&self.keyboard.state_rows())?;
        match (&self.tape, &self.tzx_tape) {
            (None, None) => writer.u8(0),
            (Some(tape), None) => {
                writer.u8(1);
                tape.write_state(&mut writer)?;
            }
            (None, Some(tape)) => {
                writer.u8(2);
                tape.write_state(&mut writer)?;
            }
            (Some(_), Some(_)) => return Err(Error::InvalidInput("multiple mounted tapes".into())),
        }
        Ok(State::sealed(INFO.id, STATE_VERSION, writer.finish()))
    }

    fn load_state(&mut self, state: &State) -> Result<()> {
        let mut reader = StateReader::new(state.checked_payload(INFO.id, STATE_VERSION)?);
        if reader.bytes()? != self.memory.rom() {
            return Err(Error::IncompatibleState("Spectrum 48K ROM mismatch".into()));
        }
        let cpu = Z80::from_state_bytes(reader.bytes()?)
            .map_err(|error| Error::IncompatibleState(error.into()))?;
        let ram = reader.bytes()?;
        if ram.len() != crate::memory::RAM_SIZE {
            return Err(Error::IncompatibleState("invalid Spectrum 48K RAM".into()));
        }
        let border_colour = reader.u8()?;
        if border_colour > 7 {
            return Err(Error::IncompatibleState(
                "invalid Spectrum border colour".into(),
            ));
        }
        let rows: [u8; 8] = reader
            .bytes()?
            .try_into()
            .map_err(|_| Error::IncompatibleState("invalid keyboard matrix size".into()))?;
        let keyboard = KeyboardMatrix::from_state_rows(rows)?;
        let (tape, tzx_tape) = match reader.u8()? {
            0 => (None, None),
            1 => (Some(TapDeck::read_state(&mut reader)?), None),
            2 => (None, Some(TzxDeck::read_state(&mut reader)?)),
            _ => {
                return Err(Error::IncompatibleState(
                    "invalid Spectrum tape state".into(),
                ))
            }
        };
        reader.finish()?;
        self.memory.load_ram(ram)?;
        self.cpu = cpu;
        self.border_colour = border_colour;
        self.keyboard = keyboard;
        self.tape = tape;
        self.tzx_tape = tzx_tape;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ROM_SIZE;

    #[test]
    fn incomplete_machine_reports_errors() {
        let mut machine: Box<dyn Machine> = Box::new(Spectrum48::new(&[0; ROM_SIZE]).unwrap());
        assert_eq!(machine.info(), INFO);
        assert!(matches!(machine.reset(), Err(Error::Unsupported(_))));
        assert!(matches!(machine.run_frame(), Err(Error::Unsupported(_))));
        assert!(machine.save_state().is_ok());
    }
}
