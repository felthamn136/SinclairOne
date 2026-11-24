use crate::{Result, VideoFrame};

/// Presentation effects must not modify emulated machine state.
pub trait VideoPresenter {
    fn present(&mut self, frame: &VideoFrame<'_>, crt_enabled: bool) -> Result<()>;
}
