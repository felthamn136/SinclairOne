use sinclair_core::{
    media::Media, services::input::InputEvent, state::StateManager, Error, Machine,
};
use sinclair_spectrum128::{memory::BANK_SIZE, Spectrum128};
use sinclair_spectrum48::{memory::ROM_SIZE, Spectrum48};

fn machine() -> Spectrum128 {
    Spectrum128::new(&[0x11; BANK_SIZE], &[0x22; BANK_SIZE]).unwrap()
}

#[test]
fn restores_all_banks_locked_paging_and_tape() {
    let mut machine = machine();
    for bank in 0..8 {
        machine.memory_mut().write_port(0x7ffd, bank);
        machine.memory_mut().write(0xc000, bank + 0x10);
        machine.memory_mut().write(0xffff, bank + 0x20);
    }
    machine.memory_mut().write_port(0x7ffd, 0x3f);
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: true,
        })
        .unwrap();
    let tzx = b"ZXTape!\x1a\x01\x14\x12\x05\x00\x02\x00";
    machine
        .load_media(&Media {
            format: "tzx",
            bytes: tzx,
        })
        .unwrap();
    machine.tzx_tape_mut().unwrap().advance(2);
    let state = machine.save_state().unwrap();
    machine.reset().unwrap();
    machine.memory_mut().write_port(0x7ffd, 7);
    machine.memory_mut().write(0xc000, 0xff);
    machine.tzx_tape_mut().unwrap().advance(10);
    machine
        .input(InputEvent {
            control: "a".into(),
            pressed: false,
        })
        .unwrap();
    machine.load_state(&state).unwrap();
    assert_eq!(machine.memory().paging_register(), 0x3f);
    assert!(machine.memory().paging_locked());
    assert_eq!(machine.memory().screen_bank(), 7);
    assert_eq!(machine.memory().read(0), 0x22);
    assert_eq!(machine.keyboard().column_mask(0xfdfe), 0x1e);
    assert_eq!(machine.tzx_tape().unwrap().remaining(), 3);
    assert_eq!(machine.save_state().unwrap(), state);
    machine.reset().unwrap();
    for bank in 0..8 {
        machine.memory_mut().write_port(0x7ffd, bank);
        assert_eq!(machine.memory().read(0xc000), bank + 0x10);
        assert_eq!(machine.memory().read(0xffff), bank + 0x20);
    }
}

#[test]
fn quick_slot_and_rolling_rewind_keep_transactional_history() {
    let mut machine = machine();
    let mut manager = StateManager::new(2);
    assert!(matches!(
        manager.quick_load(&mut machine),
        Err(Error::InvalidInput(_))
    ));
    manager.quick_save(&machine).unwrap();
    for value in 0..5 {
        machine.memory_mut().write(0xc000, value);
        manager.record(&machine).unwrap();
    }
    assert_eq!(manager.recorded(), 2);
    machine.memory_mut().write(0xc000, 5);
    let mut wrong_machine = Spectrum48::new(&[0; ROM_SIZE]).unwrap();
    assert!(manager.rewind(&mut wrong_machine).is_err());
    assert_eq!(manager.recorded(), 2);
    assert!(manager.quick_load(&mut wrong_machine).is_err());
    assert_eq!(manager.recorded(), 2);
    manager.rewind(&mut machine).unwrap();
    assert_eq!(machine.memory().read(0xc000), 4);
    manager.rewind(&mut machine).unwrap();
    assert_eq!(machine.memory().read(0xc000), 3);
    assert!(manager.rewind(&mut machine).is_err());
    manager.record(&machine).unwrap();
    machine.memory_mut().write(0xc000, 8);
    manager.quick_load(&mut machine).unwrap();
    assert_eq!(machine.memory().read(0xc000), 0);
    assert_eq!(manager.recorded(), 0);
    manager.set_history_length(1);
    manager.record(&machine).unwrap();
    machine.memory_mut().write(0xc000, 7);
    manager.record(&machine).unwrap();
    assert_eq!(manager.recorded(), 1);
    manager.set_history_length(0);
    assert_eq!(manager.recorded(), 0);
    manager.record(&machine).unwrap();
    assert_eq!(manager.recorded(), 0);
}

#[test]
fn corrupt_and_wrong_rom_states_leave_machine_unchanged() {
    let mut machine = machine();
    machine.memory_mut().write(0x4000, 0x77);
    let saved = machine.save_state().unwrap();
    let mut corrupt = saved.clone();
    corrupt.payload[100] ^= 0x80;
    assert!(machine.load_state(&corrupt).is_err());
    assert_eq!(machine.save_state().unwrap(), saved);
    let mut other = Spectrum128::new(&[0x11; BANK_SIZE], &[0; BANK_SIZE]).unwrap();
    let before = other.save_state().unwrap();
    assert!(other.load_state(&saved).is_err());
    assert_eq!(other.save_state().unwrap(), before);
}
