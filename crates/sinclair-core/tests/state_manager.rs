use sinclair_core::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateManager},
    AudioFrame, Error, Frame, Machine, MachineInfo, Result, VideoFrame,
};

struct TestMachine {
    position: u8,
    audio_phase: u8,
    video: [u8; 4],
    audio: [f32; 2],
    fail_save: bool,
    fail_frame: bool,
}

impl TestMachine {
    fn new() -> Self {
        Self {
            position: 0,
            audio_phase: 4,
            video: [0; 4],
            audio: [0.0; 2],
            fail_save: false,
            fail_frame: false,
        }
    }
}

impl Machine for TestMachine {
    fn info(&self) -> MachineInfo {
        MachineInfo {
            id: "test",
            name: "Test machine",
        }
    }

    fn reset(&mut self) -> Result<()> {
        self.position = 0;
        self.audio_phase = 4;
        Ok(())
    }

    fn input(&mut self, _: InputEvent) -> Result<()> {
        Err(Error::Unsupported("test input"))
    }

    fn run_frame(&mut self) -> Result<Frame<'_>> {
        if self.fail_frame {
            return Err(Error::Unsupported("test frame"));
        }
        self.position += 1;
        self.audio_phase += 1;
        self.video = [self.position, 0, 0, 255];
        self.audio = [f32::from(self.audio_phase) / 10.0; 2];
        Ok(Frame {
            video: VideoFrame {
                width: 1,
                height: 1,
                rgba: &self.video,
            },
            audio: AudioFrame {
                sample_rate: 44_100,
                samples: &self.audio,
            },
        })
    }

    fn load_media(&mut self, _: &Media<'_>) -> Result<()> {
        Err(Error::Unsupported("test media"))
    }

    fn save_state(&self) -> Result<State> {
        if self.fail_save {
            return Err(Error::Unsupported("test state capture"));
        }
        Ok(State::sealed(
            "test",
            1,
            vec![self.position, self.audio_phase],
        ))
    }

    fn load_state(&mut self, state: &State) -> Result<()> {
        let payload = state.checked_payload("test", 1)?;
        let [position, audio_phase] = payload else {
            return Err(Error::IncompatibleState("invalid test state".into()));
        };
        self.position = *position;
        self.audio_phase = *audio_phase;
        Ok(())
    }
}

#[test]
fn frame_recording_rewinds_and_replays_video_and_audio() {
    let mut machine = TestMachine::new();
    let mut manager = StateManager::new(2);
    for _ in 0..3 {
        manager.run_frame(&mut machine).unwrap();
    }
    assert_eq!(manager.recorded(), 2);
    manager.rewind(&mut machine).unwrap();
    let frame = manager.run_frame(&mut machine).unwrap();
    assert_eq!(frame.video.rgba, &[3, 0, 0, 255]);
    assert_eq!(frame.audio.samples, &[0.7, 0.7]);
    manager.rewind(&mut machine).unwrap();
    manager.rewind(&mut machine).unwrap();
    assert_eq!(machine.position, 1);
    assert_eq!(machine.audio_phase, 5);
    assert!(manager.rewind(&mut machine).is_err());
}

#[test]
fn quick_load_restores_independent_audio_state_for_next_frame() {
    let mut machine = TestMachine::new();
    let mut manager = StateManager::new(2);
    manager.run_frame(&mut machine).unwrap();
    manager.quick_save(&machine).unwrap();
    let frame = manager.run_frame(&mut machine).unwrap();
    let expected_video = frame.video.rgba.to_vec();
    let expected_audio = frame.audio.samples.to_vec();
    machine.audio_phase = 0;
    manager.quick_load(&mut machine).unwrap();
    assert_eq!(manager.recorded(), 0);
    let frame = manager.run_frame(&mut machine).unwrap();
    assert_eq!(frame.video.rgba, expected_video);
    assert_eq!(frame.audio.samples, expected_audio);
}

#[test]
fn failed_frames_and_captures_preserve_full_history() {
    let mut machine = TestMachine::new();
    let mut manager = StateManager::new(1);
    manager.run_frame(&mut machine).unwrap();
    machine.fail_frame = true;
    assert!(manager.run_frame(&mut machine).is_err());
    machine.fail_frame = false;
    machine.fail_save = true;
    assert!(manager.run_frame(&mut machine).is_err());
    assert_eq!(machine.position, 1);
    assert_eq!(manager.recorded(), 1);
    manager.rewind(&mut machine).unwrap();
    assert_eq!(machine.position, 0);

    manager.set_history_length(0);
    manager.run_frame(&mut machine).unwrap();
    assert_eq!(machine.position, 1);
    assert_eq!(manager.recorded(), 0);
}
