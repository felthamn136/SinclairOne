use crate::{AudioFrame, Result};

pub trait AudioOutput {
    fn submit(&mut self, frame: &AudioFrame<'_>) -> Result<()>;
}
