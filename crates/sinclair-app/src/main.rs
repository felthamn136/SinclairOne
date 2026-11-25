mod keyboard;
mod machines;

use std::path::Path;

use keyboard::{HostKeys, MacroquadKeyboard};
use machines::MachineConfig;
use macroquad::prelude::*;
use sinclair_core::{manager::MachineManager, services::input::InputManager};
use sinclair_spectrum48::keyboard::KeyboardMatrix;

#[macroquad::main("Project Sinclair")]
async fn main() {
    let config_path = Path::new("machine_config.json");
    let (mut config, mut input_error, config_writable) = match MachineConfig::load(config_path) {
        Ok(config) => (config, None, true),
        Err(error) => (
            MachineConfig::default(),
            Some(format!(
                "Configuration not loaded; it will not be overwritten: {error}"
            )),
            false,
        ),
    };
    let registry = match machines::registry(&config) {
        Ok(registry) => registry,
        Err(error) => {
            eprintln!("Cannot initialise machine registry: {error}");
            return;
        }
    };
    let mut manager = MachineManager::new(registry, config.rewind_history_length);
    if let Some(id) = &config.startup_machine {
        if let Err(error) = manager.load_machine(id) {
            input_error = Some(error.to_string());
        }
    }
    let mut input = MacroquadKeyboard::default();
    let mut matrix = KeyboardMatrix::new();
    let mut selection = 0usize;
    loop {
        let host = HostKeys {
            down: get_keys_down(),
            pressed: get_keys_pressed(),
        };
        if manager.current().is_none() {
            let entries: Vec<_> = manager
                .registry()
                .machines()
                .map(|(descriptor, available)| (descriptor.clone(), available))
                .collect();
            if !entries.is_empty() {
                if is_key_pressed(KeyCode::Up) {
                    selection = (selection + entries.len() - 1) % entries.len();
                }
                if is_key_pressed(KeyCode::Down) {
                    selection = (selection + 1) % entries.len();
                }
                if is_key_pressed(KeyCode::Enter) {
                    let _ = input.translate(&host);
                    let _ = input.release_all();
                    match machines::select_machine(
                        &mut manager,
                        &mut config,
                        config_path,
                        &entries[selection].0.id,
                        config_writable,
                    ) {
                        Ok(()) => {
                            if config_writable {
                                input_error = None;
                            }
                        }
                        Err(error) => input_error = Some(error.to_string()),
                    }
                }
            }
            clear_background(Color::from_rgba(20, 24, 32, 255));
            draw_text("Project Sinclair - Select Machine", 32.0, 64.0, 32.0, WHITE);
            draw_text(
                "Up/Down: select   Enter: load",
                32.0,
                100.0,
                20.0,
                LIGHTGRAY,
            );
            for (index, (descriptor, available)) in entries.iter().enumerate() {
                let marker = if index == selection { ">" } else { " " };
                let suffix = if *available { "" } else { " (not implemented)" };
                draw_text(
                    &format!("{marker} {}{suffix}", descriptor.name),
                    32.0,
                    145.0 + index as f32 * 30.0,
                    22.0,
                    if *available { WHITE } else { GRAY },
                );
            }
            if let Some(error) = &input_error {
                draw_text(error, 32.0, 335.0, 18.0, RED);
            }
            next_frame().await;
            continue;
        }

        let mut events = input.translate(&host);
        if host.pressed.contains(&KeyCode::Escape) {
            events.extend(input.release_all());
        }
        for event in events {
            if let Err(error) = matrix.apply_input(&event) {
                input_error = Some(error.to_string());
            }
            if let Err(error) = manager.input(event) {
                input_error = Some(error.to_string());
            }
        }
        if host.pressed.contains(&KeyCode::Escape) {
            match manager.unload_machine() {
                Ok(()) => {
                    matrix = KeyboardMatrix::new();
                    input_error = None;
                }
                Err(error) => input_error = Some(error.to_string()),
            }
            next_frame().await;
            continue;
        }

        clear_background(Color::from_rgba(20, 24, 32, 255));
        draw_text("Project Sinclair", 32.0, 64.0, 36.0, WHITE);
        if let Some(machine) = manager.current() {
            draw_text(machine.info().name, 32.0, 105.0, 24.0, LIGHTGRAY);
        }
        draw_text(
            "Architecture preview: emulation is not implemented.",
            32.0,
            140.0,
            20.0,
            YELLOW,
        );
        draw_text(
            "Spectrum keyboard preview (Esc returns to menu)",
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
