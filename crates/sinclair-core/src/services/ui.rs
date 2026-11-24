use crate::Result;

/// Host UI commands are separate from machine controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    Reset,
    OpenMedia,
    SaveState,
    LoadState,
}

pub trait UserInterface {
    fn commands(&mut self) -> Result<Vec<UiCommand>>;
    fn show_error(&mut self, message: &str);
}
