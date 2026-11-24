use std::collections::HashSet;

use macroquad::input::KeyCode;
use sinclair_core::services::input::{InputEvent, InputManager};
use sinclair_spectrum48::keyboard::Key;

pub struct HostKeys {
    pub down: HashSet<KeyCode>,
    pub pressed: HashSet<KeyCode>,
}

pub struct MacroquadKeyboard {
    active: [bool; 40],
    down: HashSet<KeyCode>,
    suppressed: HashSet<KeyCode>,
}

impl Default for MacroquadKeyboard {
    fn default() -> Self {
        Self {
            active: [false; 40],
            down: HashSet::new(),
            suppressed: HashSet::new(),
        }
    }
}

fn mapping(key: KeyCode) -> &'static [Key] {
    use KeyCode as Host;
    match key {
        Host::A => &[Key::A],
        Host::B => &[Key::B],
        Host::C => &[Key::C],
        Host::D => &[Key::D],
        Host::E => &[Key::E],
        Host::F => &[Key::F],
        Host::G => &[Key::G],
        Host::H => &[Key::H],
        Host::I => &[Key::I],
        Host::J => &[Key::J],
        Host::K => &[Key::K],
        Host::L => &[Key::L],
        Host::M => &[Key::M],
        Host::N => &[Key::N],
        Host::O => &[Key::O],
        Host::P => &[Key::P],
        Host::Q => &[Key::Q],
        Host::R => &[Key::R],
        Host::S => &[Key::S],
        Host::T => &[Key::T],
        Host::U => &[Key::U],
        Host::V => &[Key::V],
        Host::W => &[Key::W],
        Host::X => &[Key::X],
        Host::Y => &[Key::Y],
        Host::Z => &[Key::Z],
        Host::Key0 => &[Key::Num0],
        Host::Key1 => &[Key::Num1],
        Host::Key2 => &[Key::Num2],
        Host::Key3 => &[Key::Num3],
        Host::Key4 => &[Key::Num4],
        Host::Key5 => &[Key::Num5],
        Host::Key6 => &[Key::Num6],
        Host::Key7 => &[Key::Num7],
        Host::Key8 => &[Key::Num8],
        Host::Key9 => &[Key::Num9],
        Host::Enter | Host::KpEnter => &[Key::Enter],
        Host::Space => &[Key::Space],
        Host::LeftShift | Host::RightShift => &[Key::CapsShift],
        Host::LeftControl | Host::RightControl => &[Key::SymbolShift],
        Host::Backspace => &[Key::CapsShift, Key::Num0],
        Host::Left => &[Key::CapsShift, Key::Num5],
        Host::Down => &[Key::CapsShift, Key::Num6],
        Host::Up => &[Key::CapsShift, Key::Num7],
        Host::Right => &[Key::CapsShift, Key::Num8],
        Host::Comma => &[Key::SymbolShift, Key::N],
        Host::Period => &[Key::SymbolShift, Key::M],
        _ => &[],
    }
}

impl MacroquadKeyboard {
    fn update_active(&mut self, next: [bool; 40]) -> Vec<InputEvent> {
        let mut events = Vec::new();
        for key in Key::ALL {
            let index = key as usize;
            if self.active[index] != next[index] {
                events.push(InputEvent {
                    control: key.control_id().into(),
                    pressed: next[index],
                });
            }
        }
        self.active = next;
        events
    }
}

impl InputManager for MacroquadKeyboard {
    type HostEvent = HostKeys;

    fn translate(&mut self, host: &HostKeys) -> Vec<InputEvent> {
        self.suppressed
            .retain(|key| host.down.contains(key) && !host.pressed.contains(key));
        self.down.clone_from(&host.down);
        let mut next = [false; 40];
        for host_key in host.down.difference(&self.suppressed) {
            for key in mapping(*host_key) {
                next[*key as usize] = true;
            }
        }
        self.update_active(next)
    }

    fn release_all(&mut self) -> Vec<InputEvent> {
        self.suppressed.extend(self.down.iter().copied());
        self.update_active([false; 40])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sinclair_spectrum48::keyboard::KeyboardMatrix;

    fn keys(down: &[KeyCode], pressed: &[KeyCode]) -> HostKeys {
        HostKeys {
            down: down.iter().copied().collect(),
            pressed: pressed.iter().copied().collect(),
        }
    }

    fn update(adapter: &mut MacroquadKeyboard, matrix: &mut KeyboardMatrix, keys: &HostKeys) {
        for event in adapter.translate(keys) {
            matrix.apply_input(&event).unwrap();
        }
    }

    #[test]
    fn shifts_and_shortcuts_share_keys_without_premature_release() {
        let mut adapter = MacroquadKeyboard::default();
        let mut matrix = KeyboardMatrix::new();
        update(
            &mut adapter,
            &mut matrix,
            &keys(&[KeyCode::LeftShift, KeyCode::Backspace], &[]),
        );
        assert_eq!(matrix.column_mask(0xfefe), 0x1e);
        assert_eq!(matrix.column_mask(0xeffe), 0x1e);
        update(&mut adapter, &mut matrix, &keys(&[KeyCode::LeftShift], &[]));
        assert_eq!(matrix.column_mask(0xfefe), 0x1e);
        assert_eq!(matrix.column_mask(0xeffe), 0x1f);
        update(&mut adapter, &mut matrix, &keys(&[], &[]));
        assert_eq!(matrix.column_mask(0xfefe), 0x1f);
    }

    #[test]
    fn shifted_letter_maps_to_caps_shift_and_the_letter() {
        let mut adapter = MacroquadKeyboard::default();
        let mut matrix = KeyboardMatrix::new();
        update(
            &mut adapter,
            &mut matrix,
            &keys(&[KeyCode::RightShift, KeyCode::A], &[]),
        );
        assert_eq!(matrix.column_mask(0xfefe), 0x1e);
        assert_eq!(matrix.column_mask(0xfdfe), 0x1e);
    }

    #[test]
    fn symbol_shortcut_and_control_key_compose() {
        let mut adapter = MacroquadKeyboard::default();
        let mut matrix = KeyboardMatrix::new();
        update(
            &mut adapter,
            &mut matrix,
            &keys(&[KeyCode::RightControl, KeyCode::Period], &[]),
        );
        assert_eq!(matrix.column_mask(0x7ffe), 0x19);
        update(
            &mut adapter,
            &mut matrix,
            &keys(&[KeyCode::RightControl], &[]),
        );
        assert_eq!(matrix.column_mask(0x7ffe), 0x1d);
    }

    #[test]
    fn release_all_suppresses_stale_host_state_until_a_new_press() {
        let mut adapter = MacroquadKeyboard::default();
        let mut matrix = KeyboardMatrix::new();
        let down = keys(&[KeyCode::A], &[KeyCode::A]);
        update(&mut adapter, &mut matrix, &down);
        for event in adapter.release_all() {
            matrix.apply_input(&event).unwrap();
        }
        update(&mut adapter, &mut matrix, &keys(&[KeyCode::A], &[]));
        assert_eq!(matrix.column_mask(0xfdfe), 0x1f);
        update(&mut adapter, &mut matrix, &down);
        assert_eq!(matrix.column_mask(0xfdfe), 0x1e);
    }
}
