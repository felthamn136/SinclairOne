/// A machine-defined control identifier and its new state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEvent {
    pub control: String,
    pub pressed: bool,
}

/// Maps host input to the selected machine's control identifiers.
pub trait InputManager {
    type HostEvent;

    fn translate(&mut self, event: &Self::HostEvent) -> Vec<InputEvent>;
    /// Called on focus loss or machine replacement to prevent stuck controls.
    fn release_all(&mut self) -> Vec<InputEvent>;
}
