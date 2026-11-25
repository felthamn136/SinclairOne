use std::{cell::Cell, rc::Rc};

use sinclair_core::{
    manager::{MachineDescriptor, MachineFactory, MachineManager, MachineRegistry},
    media::Media,
    services::input::InputEvent,
    state::State,
    AudioFrame, Error, Frame, Machine, MachineInfo, Result, VideoFrame,
};

struct TestMachine {
    id: &'static str,
    value: u8,
    video: [u8; 4],
    audio: [f32; 2],
    media: bool,
    fail_shutdown: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for TestMachine {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

impl Machine for TestMachine {
    fn info(&self) -> MachineInfo {
        MachineInfo {
            id: self.id,
            name: self.id,
        }
    }
    fn reset(&mut self) -> Result<()> {
        self.value = 0;
        Ok(())
    }
    fn input(&mut self, event: InputEvent) -> Result<()> {
        self.value = u8::from(event.pressed);
        Ok(())
    }
    fn run_frame(&mut self) -> Result<Frame<'_>> {
        self.value += 1;
        self.video[0] = self.value;
        Ok(Frame {
            video: VideoFrame {
                width: 1,
                height: 1,
                rgba: &self.video,
            },
            audio: AudioFrame {
                sample_rate: 1,
                samples: &self.audio,
            },
        })
    }
    fn load_media(&mut self, media: &Media<'_>) -> Result<()> {
        if media.format != "tap" {
            return Err(Error::Unsupported("test format"));
        }
        self.media = true;
        Ok(())
    }
    fn eject_media(&mut self) -> Result<()> {
        self.media = false;
        Ok(())
    }
    fn media_information(&self) -> Result<Option<&'static str>> {
        Ok(self.media.then_some("tap"))
    }
    fn shutdown(&mut self) -> Result<()> {
        if self.fail_shutdown.get() {
            Err(Error::InvalidInput("shutdown failed".into()))
        } else {
            Ok(())
        }
    }
    fn save_state(&self) -> Result<State> {
        Ok(State::sealed(
            self.id,
            1,
            vec![self.value, u8::from(self.media)],
        ))
    }
    fn load_state(&mut self, state: &State) -> Result<()> {
        let payload = state.checked_payload(self.id, 1)?;
        let [value, media @ 0..=1] = payload else {
            return Err(Error::IncompatibleState("invalid test state".into()));
        };
        self.value = *value;
        self.media = *media != 0;
        Ok(())
    }
}

fn descriptor(id: &str) -> MachineDescriptor {
    MachineDescriptor {
        id: id.into(),
        name: id.into(),
        manufacturer: "test".into(),
        year: 1981,
        cpu: "test".into(),
        memory: "test".into(),
    }
}

fn factory(
    id: &'static str,
    fail_shutdown: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
) -> Box<dyn MachineFactory> {
    Box::new(move || -> Result<Box<dyn Machine>> {
        Ok(Box::new(TestMachine {
            id,
            value: 0,
            video: [0; 4],
            audio: [0.0; 2],
            media: false,
            fail_shutdown: fail_shutdown.clone(),
            drops: drops.clone(),
        }))
    })
}

#[test]
fn registry_and_lifecycle_preserve_active_machine_on_failure() {
    let fail = Rc::new(Cell::new(false));
    let drops = Rc::new(Cell::new(0));
    let mut registry = MachineRegistry::new();
    registry
        .register(
            descriptor("one"),
            Some(factory("one", fail.clone(), drops.clone())),
        )
        .unwrap();
    registry
        .register(
            descriptor("two"),
            Some(factory("two", fail.clone(), drops.clone())),
        )
        .unwrap();
    registry.register(descriptor("planned"), None).unwrap();
    registry
        .register(
            descriptor("bad-factory"),
            Some(factory("wrong-id", fail.clone(), drops.clone())),
        )
        .unwrap();
    assert!(registry.register(descriptor("one"), None).is_err());
    assert_eq!(registry.machines().count(), 4);
    let mut manager = MachineManager::new(registry, 2);
    manager.load_machine("one").unwrap();
    assert!(manager.switch_machine("unknown").is_err());
    assert!(manager.switch_machine("planned").is_err());
    assert!(manager.switch_machine("bad-factory").is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(manager.current().unwrap().info().id, "one");
    fail.set(true);
    assert!(manager.switch_machine("two").is_err());
    assert_eq!(manager.current().unwrap().info().id, "one");
    assert_eq!(drops.get(), 2);
    assert!(manager.unload_machine().is_err());
    fail.set(false);
    manager.switch_machine("two").unwrap();
    assert_eq!(manager.current().unwrap().info().id, "two");
    assert_eq!(drops.get(), 3);
    manager.restart_machine().unwrap();
    assert_eq!(drops.get(), 4);
    manager.unload_machine().unwrap();
    assert!(manager.current().is_none());
    assert_eq!(drops.get(), 5);
    assert!(manager.reset_machine().is_err());
}

#[test]
fn media_paths_use_the_case_insensitive_extension() {
    let path = std::env::temp_dir().join(format!("sinclair-manager-{}.TAP", std::process::id()));
    std::fs::write(&path, [1]).unwrap();
    let mut registry = MachineRegistry::new();
    registry
        .register(
            descriptor("one"),
            Some(factory(
                "one",
                Rc::new(Cell::new(false)),
                Rc::new(Cell::new(0)),
            )),
        )
        .unwrap();
    let mut manager = MachineManager::new(registry, 0);
    manager.load_machine("one").unwrap();
    manager.load_media_path(&path).unwrap();
    assert_eq!(manager.media_information().unwrap(), Some("tap"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn quick_state_rewind_reset_and_media_are_routed_to_active_machine() {
    let mut registry = MachineRegistry::new();
    registry
        .register(
            descriptor("one"),
            Some(factory(
                "one",
                Rc::new(Cell::new(false)),
                Rc::new(Cell::new(0)),
            )),
        )
        .unwrap();
    registry
        .register(
            descriptor("two"),
            Some(factory(
                "two",
                Rc::new(Cell::new(false)),
                Rc::new(Cell::new(0)),
            )),
        )
        .unwrap();
    let mut manager = MachineManager::new(registry, 2);
    manager.load_machine("one").unwrap();
    manager
        .input(InputEvent {
            control: "x".into(),
            pressed: true,
        })
        .unwrap();
    manager.quick_save().unwrap();
    assert_eq!(manager.run_frame().unwrap().video.rgba[0], 2);
    manager.rewind().unwrap();
    assert_eq!(manager.run_frame().unwrap().video.rgba[0], 2);
    manager.quick_load().unwrap();
    assert_eq!(manager.run_frame().unwrap().video.rgba[0], 2);
    manager
        .load_media(&Media {
            format: "tap",
            bytes: &[],
        })
        .unwrap();
    assert_eq!(manager.media_information().unwrap(), Some("tap"));
    let state = manager.save_state().unwrap();
    manager.eject_media().unwrap();
    assert_eq!(manager.media_information().unwrap(), None);
    manager.load_state(&state).unwrap();
    assert_eq!(manager.media_information().unwrap(), Some("tap"));
    manager.reset_machine().unwrap();
    assert_eq!(manager.run_frame().unwrap().video.rgba[0], 1);
    manager.switch_machine("two").unwrap();
    assert!(manager.quick_load().is_err());
    assert!(manager.load_state(&state).is_err());
}
