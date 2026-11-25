use crate::{media::Media, services::input::InputEvent, state::State, Result};

/// Stable identifier values must not change once states are persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineInfo {
    pub id: &'static str,
    pub name: &'static str,
}

/// Row-major RGBA8 pixels; length must equal width * height * 4.
pub struct VideoFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// Interleaved stereo samples in the range -1.0..=1.0.
pub struct AudioFrame<'a> {
    pub sample_rate: u32,
    pub samples: &'a [f32],
}

pub struct Frame<'a> {
    pub video: VideoFrame<'a>,
    pub audio: AudioFrame<'a>,
}

/// A host schedules machines without knowing their CPU or hardware.
pub trait Machine {
    fn info(&self) -> MachineInfo;
    fn reset(&mut self) -> Result<()>;
    fn input(&mut self, event: InputEvent) -> Result<()>;
    fn run_frame(&mut self) -> Result<Frame<'_>>;
    fn load_media(&mut self, media: &Media<'_>) -> Result<()>;
    /// Release resources before the machine is removed. Failure keeps it active.
    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
    /// Eject removable media, if supported by this machine.
    fn eject_media(&mut self) -> Result<()> {
        Err(crate::Error::Unsupported("media ejection"))
    }
    /// Return the format of mounted removable media, if any.
    fn media_information(&self) -> Result<Option<&'static str>> {
        Err(crate::Error::Unsupported("media information"))
    }
    fn save_state(&self) -> Result<State>;
    /// Validate fully before changing the machine; errors leave it unchanged.
    fn load_state(&mut self, state: &State) -> Result<()>;
}
