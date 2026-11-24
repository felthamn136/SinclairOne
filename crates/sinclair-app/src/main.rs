mod keyboard;

use keyboard::{HostKeys, MacroquadKeyboard};
use macroquad::prelude::*;
use sinclair_core::services::input::InputManager;
use sinclair_spectrum48::{keyboard::KeyboardMatrix, machine::INFO};

#[macroquad::main("Project Sinclair")]
async fn main() {
    let mut input = MacroquadKeyboard::default();
    let mut matrix = KeyboardMatrix::new();
    let mut input_error = None;
    loop {
        let host = HostKeys {
            down: get_keys_down(),
            pressed: get_keys_pressed(),
        };
        let mut events = input.translate(&host);
        if host.pressed.contains(&KeyCode::Escape) {
            events.extend(input.release_all());
        }
        for event in events {
            if let Err(error) = matrix.apply_input(&event) {
                input_error = Some(error.to_string());
            }
        }

        clear_background(Color::from_rgba(20, 24, 32, 255));
        draw_text("Project Sinclair", 32.0, 64.0, 36.0, WHITE);
        draw_text(INFO.name, 32.0, 105.0, 24.0, LIGHTGRAY);
        draw_text(
            "Architecture preview: emulation is not implemented.",
            32.0,
            140.0,
            20.0,
            YELLOW,
        );
        draw_text(
            "Keyboard matrix preview (Esc releases keys)",
            32.0,
            184.0,
            20.0,
            LIGHTGRAY,
        );
        draw_text(
            "Shift = CAPS SHIFT; Ctrl = SYMBOL SHIFT",
            32.0,
            210.0,
            20.0,
            LIGHTGRAY,
        );
        for row in 0..8 {
            let port = 0xfffe & !(1 << (row + 8));
            let text = format!("A{}: {:02X}", row + 8, matrix.column_mask(port));
            draw_text(
                &text,
                32.0 + (row % 4) as f32 * 125.0,
                248.0 + (row / 4) as f32 * 28.0,
                20.0,
                WHITE,
            );
        }
        if let Some(error) = &input_error {
            draw_text(error, 32.0, 320.0, 20.0, RED);
        }
        next_frame().await;
    }
}
