//! ZX Spectrum 48K screen-memory and border renderer.
//!
//! This is a snapshot renderer, not a cycle-accurate ULA: the machine supplies
//! a frame number, the initial border colour, and beam-positioned changes.

use sinclair_core::{Error, Result};

use crate::memory::Memory;

pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 192;
pub const BORDER_LEFT: usize = 32;
pub const BORDER_TOP: usize = 24;
pub const FRAME_WIDTH: usize = SCREEN_WIDTH + BORDER_LEFT * 2;
pub const FRAME_HEIGHT: usize = SCREEN_HEIGHT + BORDER_TOP * 2;
pub const FRAME_RGBA_BYTES: usize = FRAME_WIDTH * FRAME_HEIGHT * 4;
pub const FLASH_HALF_PERIOD_FRAMES: u64 = 16;

/// Conventional RGB approximations; Spectrum colour numbers encode B, R, G.
pub const PALETTE: [[[u8; 4]; 8]; 2] = [
    [
        [0x00, 0x00, 0x00, 0xff],
        [0x00, 0x00, 0xcd, 0xff],
        [0xcd, 0x00, 0x00, 0xff],
        [0xcd, 0x00, 0xcd, 0xff],
        [0x00, 0xcd, 0x00, 0xff],
        [0x00, 0xcd, 0xcd, 0xff],
        [0xcd, 0xcd, 0x00, 0xff],
        [0xcd, 0xcd, 0xcd, 0xff],
    ],
    [
        [0x00, 0x00, 0x00, 0xff],
        [0x00, 0x00, 0xff, 0xff],
        [0xff, 0x00, 0x00, 0xff],
        [0xff, 0x00, 0xff, 0xff],
        [0x00, 0xff, 0x00, 0xff],
        [0x00, 0xff, 0xff, 0xff],
        [0xff, 0xff, 0x00, 0xff],
        [0xff, 0xff, 0xff, 0xff],
    ],
];

/// Border colour changes immediately before the pixel at `(x, y)` in the
/// cropped frame. Equal positions are allowed and retain input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderChange {
    pub x: usize,
    pub y: usize,
    pub colour: u8,
}

impl BorderChange {
    /// Create an event from a ULA port write; only bits 0–2 select the border.
    pub fn from_port(x: usize, y: usize, value: u8) -> Self {
        Self {
            x,
            y,
            colour: value & 7,
        }
    }
}

/// Rendering inputs for one frame; frame zero is the non-inverted FLASH phase.
pub struct FrameConfig<'a> {
    pub frame_number: u64,
    pub border_colour: u8,
    pub border_changes: &'a [BorderChange],
}

/// Returns the bitmap byte address for a screen pixel (not a character cell).
pub fn screen_address(x: usize, y: usize) -> Result<u16> {
    validate_screen_pixel(x, y)?;
    Ok(bitmap_address(x, y))
}

/// Returns the attribute byte address for a screen pixel.
pub fn attribute_address(x: usize, y: usize) -> Result<u16> {
    validate_screen_pixel(x, y)?;
    Ok(attribute_address_unchecked(x, y))
}

fn validate_screen_pixel(x: usize, y: usize) -> Result<()> {
    if x >= SCREEN_WIDTH || y >= SCREEN_HEIGHT {
        return Err(Error::InvalidInput(format!(
            "screen pixel ({x}, {y}) is outside {SCREEN_WIDTH}x{SCREEN_HEIGHT}"
        )));
    }
    Ok(())
}

fn bitmap_address(x: usize, y: usize) -> u16 {
    (0x4000 | ((y & 0xc0) << 5) | ((y & 0x07) << 8) | ((y & 0x38) << 2) | (x >> 3)) as u16
}

fn attribute_address_unchecked(x: usize, y: usize) -> u16 {
    (0x5800 + (y >> 3) * 32 + (x >> 3)) as u16
}

fn validate_config(config: &FrameConfig<'_>) -> Result<()> {
    if config.border_colour > 7 {
        return Err(Error::InvalidInput(format!(
            "invalid border colour {}",
            config.border_colour
        )));
    }
    let mut previous = None;
    for event in config.border_changes {
        if event.x >= FRAME_WIDTH || event.y >= FRAME_HEIGHT || event.colour > 7 {
            return Err(Error::InvalidInput(format!(
                "invalid border change at ({}, {}) with colour {}",
                event.x, event.y, event.colour
            )));
        }
        let position = (event.y, event.x);
        if previous.is_some_and(|last| position < last) {
            return Err(Error::InvalidInput(
                "border changes must be in raster order".into(),
            ));
        }
        previous = Some(position);
    }
    Ok(())
}

/// CPU-independent renderer using the machine's 48K memory map.
#[derive(Debug, Default, Clone, Copy)]
pub struct Ula;

impl Ula {
    pub fn new() -> Self {
        Self
    }

    /// Render a single cropped-frame row as `FRAME_WIDTH` RGBA8 pixels.
    pub fn render_scanline(
        &self,
        memory: &Memory,
        config: &FrameConfig<'_>,
        y: usize,
    ) -> Result<Vec<u8>> {
        validate_config(config)?;
        if y >= FRAME_HEIGHT {
            return Err(Error::InvalidInput(format!(
                "scanline {y} is outside frame height {FRAME_HEIGHT}"
            )));
        }
        let mut colour = config.border_colour;
        let mut event_index = 0;
        while event_index < config.border_changes.len() && config.border_changes[event_index].y < y
        {
            colour = config.border_changes[event_index].colour;
            event_index += 1;
        }
        let mut pixels = vec![0; FRAME_WIDTH * 4];
        render_row(
            memory,
            config,
            y,
            &mut colour,
            &mut event_index,
            &mut pixels,
        );
        Ok(pixels)
    }

    /// Render a complete cropped frame as row-major RGBA8 pixels.
    pub fn render_frame(&self, memory: &Memory, config: &FrameConfig<'_>) -> Result<Vec<u8>> {
        validate_config(config)?;
        let mut pixels = vec![0; FRAME_RGBA_BYTES];
        let mut colour = config.border_colour;
        let mut event_index = 0;
        for (y, row) in pixels.chunks_mut(FRAME_WIDTH * 4).enumerate() {
            render_row(memory, config, y, &mut colour, &mut event_index, row);
        }
        Ok(pixels)
    }
}

fn render_row(
    memory: &Memory,
    config: &FrameConfig<'_>,
    y: usize,
    colour: &mut u8,
    event_index: &mut usize,
    pixels: &mut [u8],
) {
    let active_y = y.checked_sub(BORDER_TOP).filter(|&y| y < SCREEN_HEIGHT);
    let flash_phase = (config.frame_number / FLASH_HALF_PERIOD_FRAMES) & 1 != 0;
    let mut bitmap = 0;
    let mut attribute = 0;
    for (x, pixel) in pixels.chunks_mut(4).enumerate() {
        while let Some(event) = config.border_changes.get(*event_index) {
            if (event.y, event.x) != (y, x) {
                break;
            }
            *colour = event.colour;
            *event_index += 1;
        }
        let active_x = x.checked_sub(BORDER_LEFT).filter(|&x| x < SCREEN_WIDTH);
        let rgba = match (active_x, active_y) {
            (Some(screen_x), Some(screen_y)) => {
                if screen_x & 7 == 0 {
                    bitmap = memory.read(bitmap_address(screen_x, screen_y));
                    attribute = memory.read(attribute_address_unchecked(screen_x, screen_y));
                }
                let bit_set = bitmap & (0x80 >> (screen_x & 7)) != 0;
                let ink = attribute & 7;
                let paper = (attribute >> 3) & 7;
                let invert = attribute & 0x80 != 0 && flash_phase;
                let colour_index = if bit_set ^ invert { ink } else { paper };
                PALETTE[usize::from((attribute >> 6) & 1)][usize::from(colour_index)]
            }
            _ => PALETTE[0][usize::from(*colour)],
        };
        pixel.copy_from_slice(&rgba);
    }
}
