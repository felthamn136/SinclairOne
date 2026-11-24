//! Z80-specific CPU and bus; not a generic interface imposed on other CPUs.

pub mod bus;
pub mod cpu;

mod base;
mod cb;
mod ed;

pub use bus::Bus;
pub use cpu::{InterruptMode, Z80};
