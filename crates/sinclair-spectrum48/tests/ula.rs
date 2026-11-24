use sinclair_core::Error;
use sinclair_spectrum48::{
    memory::{Memory, ROM_SIZE},
    video::{
        attribute_address, screen_address, BorderChange, FrameConfig, Ula, BORDER_LEFT, BORDER_TOP,
        FRAME_HEIGHT, FRAME_RGBA_BYTES, FRAME_WIDTH, SCREEN_HEIGHT, SCREEN_WIDTH,
    },
};

fn memory() -> Memory {
    Memory::new(&[0; ROM_SIZE]).unwrap()
}

fn pixel(frame: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * FRAME_WIDTH + x) * 4;
    frame[offset..offset + 4].try_into().unwrap()
}

#[test]
fn address_layout_covers_screen_and_attributes() {
    for (x, y, bitmap, attribute) in [
        (0, 0, 0x4000, 0x5800),
        (7, 7, 0x4700, 0x5800),
        (8, 0, 0x4001, 0x5801),
        (0, 8, 0x4020, 0x5820),
        (0, 63, 0x47e0, 0x58e0),
        (0, 64, 0x4800, 0x5900),
        (0, 128, 0x5000, 0x5a00),
        (255, 191, 0x57ff, 0x5aff),
    ] {
        assert_eq!(screen_address(x, y).unwrap(), bitmap);
        assert_eq!(attribute_address(x, y).unwrap(), attribute);
    }
    let mut addresses = std::collections::BTreeSet::new();
    for y in 0..SCREEN_HEIGHT {
        for x in (0..SCREEN_WIDTH).step_by(8) {
            addresses.insert(screen_address(x, y).unwrap());
        }
    }
    assert_eq!(addresses.len(), 6144);
    assert!(matches!(
        screen_address(256, 0),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        attribute_address(0, 192),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn ink_paper_and_bright_render_all_colours_msb_first() {
    let mut memory = memory();
    let expected = [
        [0x00, 0x00, 0x00, 0xff],
        [0x00, 0x00, 0xcd, 0xff],
        [0xcd, 0x00, 0x00, 0xff],
        [0xcd, 0x00, 0xcd, 0xff],
        [0x00, 0xcd, 0x00, 0xff],
        [0x00, 0xcd, 0xcd, 0xff],
        [0xcd, 0xcd, 0x00, 0xff],
        [0xcd, 0xcd, 0xcd, 0xff],
        [0x00, 0x00, 0x00, 0xff],
        [0x00, 0x00, 0xff, 0xff],
        [0xff, 0x00, 0x00, 0xff],
        [0xff, 0x00, 0xff, 0xff],
        [0x00, 0xff, 0x00, 0xff],
        [0x00, 0xff, 0xff, 0xff],
        [0xff, 0xff, 0x00, 0xff],
        [0xff, 0xff, 0xff, 0xff],
    ];
    for bright in 0..2 {
        for ink in 0..8 {
            let column = bright * 8 + ink;
            let paper = 7 - ink;
            memory.write(0x4000 + column as u16, 0x80);
            memory.write(
                0x5800 + column as u16,
                (bright << 6 | paper << 3 | ink) as u8,
            );
        }
    }
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 0,
        border_changes: &[],
    };
    let row = Ula::new()
        .render_scanline(&memory, &config, BORDER_TOP)
        .unwrap();
    for bright in 0..2 {
        for ink in 0..8 {
            let column = bright * 8 + ink;
            let x = BORDER_LEFT + column * 8;
            assert_eq!(&row[x * 4..x * 4 + 4], &expected[bright * 8 + ink]);
            assert_eq!(
                &row[(x + 1) * 4..(x + 1) * 4 + 4],
                &expected[bright * 8 + 7 - ink]
            );
        }
    }
}

#[test]
fn flash_swaps_ink_and_paper_every_sixteen_frames() {
    let mut memory = memory();
    memory.write(0x4000, 0x80);
    memory.write(0x5800, 0x80 | (2 << 3) | 1);
    memory.write(0x4001, 0x80);
    memory.write(0x5801, (2 << 3) | 1);
    for (frame_number, ink, paper) in [
        (0, [0, 0, 0xcd, 255], [0xcd, 0, 0, 255]),
        (15, [0, 0, 0xcd, 255], [0xcd, 0, 0, 255]),
        (16, [0xcd, 0, 0, 255], [0, 0, 0xcd, 255]),
        (31, [0xcd, 0, 0, 255], [0, 0, 0xcd, 255]),
        (32, [0, 0, 0xcd, 255], [0xcd, 0, 0, 255]),
    ] {
        let config = FrameConfig {
            frame_number,
            border_colour: 0,
            border_changes: &[],
        };
        let row = Ula::new()
            .render_scanline(&memory, &config, BORDER_TOP)
            .unwrap();
        let x = BORDER_LEFT;
        assert_eq!(&row[x * 4..x * 4 + 4], &ink);
        assert_eq!(&row[(x + 1) * 4..(x + 1) * 4 + 4], &paper);
        assert_eq!(&row[(x + 8) * 4..(x + 8) * 4 + 4], &[0, 0, 0xcd, 255]);
    }
}

#[test]
fn border_changes_apply_at_pixel_and_persist_across_lines() {
    let memory = memory();
    let changes = [
        BorderChange::from_port(2, 0, 0xfa),
        BorderChange::from_port(2, 0, 0xfc),
        BorderChange::from_port(100, BORDER_TOP, 1),
        BorderChange::from_port(1, FRAME_HEIGHT - 1, 6),
    ];
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 0,
        border_changes: &changes,
    };
    let ula = Ula::new();
    let frame = ula.render_frame(&memory, &config).unwrap();
    assert_eq!(frame.len(), FRAME_RGBA_BYTES);
    assert_eq!(pixel(&frame, 1, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&frame, 2, 0), [0, 0xcd, 0, 255]);
    assert_eq!(pixel(&frame, 0, BORDER_TOP), [0, 0xcd, 0, 255]);
    assert_eq!(
        pixel(&frame, FRAME_WIDTH - 1, BORDER_TOP),
        [0, 0, 0xcd, 255]
    );
    assert_eq!(pixel(&frame, 0, BORDER_TOP + 1), [0, 0, 0xcd, 255]);
    assert_eq!(pixel(&frame, 0, FRAME_HEIGHT - 1), [0, 0, 0xcd, 255]);
    assert_eq!(pixel(&frame, 1, FRAME_HEIGHT - 1), [0xcd, 0xcd, 0, 255]);
    for y in [0, BORDER_TOP, BORDER_TOP + 1, FRAME_HEIGHT - 1] {
        let row = ula.render_scanline(&memory, &config, y).unwrap();
        assert_eq!(&row, &frame[y * FRAME_WIDTH * 4..(y + 1) * FRAME_WIDTH * 4]);
    }
}

#[test]
fn invalid_render_inputs_are_rejected() {
    let memory = memory();
    let ula = Ula::new();
    let invalid = [
        BorderChange {
            x: 2,
            y: 1,
            colour: 0,
        },
        BorderChange {
            x: 1,
            y: 1,
            colour: 0,
        },
    ];
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 0,
        border_changes: &invalid,
    };
    assert!(matches!(
        ula.render_frame(&memory, &config),
        Err(Error::InvalidInput(_))
    ));
    for event in [
        BorderChange {
            x: FRAME_WIDTH,
            y: 0,
            colour: 0,
        },
        BorderChange {
            x: 0,
            y: FRAME_HEIGHT,
            colour: 0,
        },
        BorderChange {
            x: 0,
            y: 0,
            colour: 8,
        },
    ] {
        let config = FrameConfig {
            frame_number: 0,
            border_colour: 0,
            border_changes: std::slice::from_ref(&event),
        };
        assert!(matches!(
            ula.render_frame(&memory, &config),
            Err(Error::InvalidInput(_))
        ));
    }
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 8,
        border_changes: &[],
    };
    assert!(matches!(
        ula.render_frame(&memory, &config),
        Err(Error::InvalidInput(_))
    ));
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 0,
        border_changes: &[],
    };
    assert!(matches!(
        ula.render_scanline(&memory, &config, FRAME_HEIGHT),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn renders_a_complete_loaded_scr_image() {
    let mut memory = memory();
    let mut scr = [0u8; 6912];
    for third in 0..3 {
        for character_row in 0..8 {
            for line_in_character in 0..8 {
                for column in 0..32 {
                    let y = third * 64 + character_row * 8 + line_in_character;
                    let offset =
                        third * 2048 + line_in_character * 256 + character_row * 32 + column;
                    scr[offset] = if (y + column) % 2 == 0 { 0xaa } else { 0x55 };
                }
            }
        }
    }
    scr[6144..].fill(0x47); // bright white ink, black paper
    for (offset, &value) in scr.iter().enumerate() {
        memory.write(0x4000 + offset as u16, value);
    }
    let config = FrameConfig {
        frame_number: 0,
        border_colour: 2,
        border_changes: &[],
    };
    let frame = Ula::new().render_frame(&memory, &config).unwrap();
    for y in 0..SCREEN_HEIGHT {
        for x in 0..SCREEN_WIDTH {
            let byte = if (y + x / 8) % 2 == 0 { 0xaa } else { 0x55 };
            let expected = if byte & (0x80 >> (x % 8)) != 0 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(
                pixel(&frame, BORDER_LEFT + x, BORDER_TOP + y),
                expected,
                "screen ({x}, {y})"
            );
        }
    }
    assert_eq!(pixel(&frame, 0, 0), [0xcd, 0, 0, 255]);
}
