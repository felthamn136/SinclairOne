//! Machine-neutral contracts shared by emulator implementations and hosts.

pub mod error;
pub mod machine;
pub mod manager;
pub mod media;
pub mod services;
pub mod state;

pub use error::{Error, Result};
pub use machine::{AudioFrame, Frame, Machine, MachineInfo, VideoFrame};
