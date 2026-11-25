use sinclair_core::{Error, Result};

use crate::memory::{Memory, RAM_START};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 192;
const CHARACTER_SET: u16 = 0x1e00;
const NEWLINE: u8 = 0x76;

/// Render a conventional ROM display file; this is not cycle-timed ULA video.
pub fn render_display(memory: &Memory) -> Result<Vec<u8>> {
    let start = u16::from_le_bytes([memory.read(0x400c), memory.read(0x400d)]);
    let end = usize::from(RAM_START) + memory.ram().len();
    let mut address = usize::from(start);
    if address < usize::from(RAM_START) || address >= end || memory.read(start) != NEWLINE {
        return Err(Error::InvalidInput("invalid ZX81 display file".into()));
    }
    address += 1;
    let mut pixels = vec![255; WIDTH * HEIGHT * 4];
    for row in 0..24 {
        let mut column = 0;
        loop {
            if address >= end {
                return Err(Error::InvalidInput("truncated ZX81 display file".into()));
            }
            let code = memory.read(address as u16);
            address += 1;
            if code == NEWLINE {
                break;
            }
            if column == 32 || code & 0x40 != 0 {
                return Err(Error::InvalidInput("invalid ZX81 display row".into()));
            }
            for y in 0..8 {
                let glyph = memory.read(CHARACTER_SET + u16::from(code & 0x3f) * 8 + y);
                for x in 0..8 {
                    let black = ((glyph >> (7 - x)) & 1 != 0) ^ (code & 0x80 != 0);
                    let intensity = if black { 0 } else { 255 };
                    let offset = ((row * 8 + usize::from(y)) * WIDTH + column * 8 + x) * 4;
                    pixels[offset..offset + 3].fill(intensity);
                }
            }
            column += 1;
        }
    }
    Ok(pixels)
}
