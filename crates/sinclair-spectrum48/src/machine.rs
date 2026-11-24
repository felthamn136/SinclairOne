use sinclair_core::{
    media::Media, services::input::InputEvent, state::State, Error, Frame, Machine, MachineInfo,
    Result,
};
use sinclair_cpu_z80::Z80;

use crate::{
    keyboard::KeyboardMatrix,
    media::tap::{TapDeck, TapImage},
    memory::Memory,
};

pub const INFO: MachineInfo = MachineInfo {
    id: "spectrum48",
    name: "ZX Spectrum 48K",
};

pub struct Spectrum48 {
    // Components are retained here until machine scheduling and I/O are implemented.
    _cpu: Z80,
    _memory: Memory,
    keyboard: KeyboardMatrix,
    tape: Option<TapDeck>,
}

impl Spectrum48 {
    pub fn new(rom: &[u8]) -> Result<Self> {
        Ok(Self {
            _cpu: Z80::new(),
            _memory: Memory::new(rom)?,
            keyboard: KeyboardMatrix::new(),
            tape: None,
        })
    }

    pub fn keyboard(&self) -> &KeyboardMatrix {
        &self.keyboard
    }

    pub fn tape(&self) -> Option<&TapDeck> {
        self.tape.as_ref()
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
        if media.format != "tap" {
            return Err(Error::Unsupported("Spectrum 48K media format"));
        }
        let image = TapImage::parse(media.bytes)?;
        self.tape = Some(TapDeck::new(image));
        Ok(())
    }

    fn save_state(&self) -> Result<State> {
        Err(Error::Unsupported("Spectrum 48K save state"))
    }

    fn load_state(&mut self, _state: &State) -> Result<()> {
        Err(Error::Unsupported("Spectrum 48K load state"))
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
        assert!(matches!(machine.save_state(), Err(Error::Unsupported(_))));
    }
}
