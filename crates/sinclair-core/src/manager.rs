//! Machine-neutral registration, lifecycle and state management.

use std::{fs, path::Path};

use crate::{
    media::Media,
    services::input::InputEvent,
    state::{State, StateManager},
    Error, Frame, Machine, Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineDescriptor {
    pub id: String,
    pub name: String,
    pub manufacturer: String,
    pub year: u16,
    pub cpu: String,
    pub memory: String,
}

pub trait MachineFactory {
    fn create(&self) -> Result<Box<dyn Machine>>;
}

impl<F> MachineFactory for F
where
    F: Fn() -> Result<Box<dyn Machine>>,
{
    fn create(&self) -> Result<Box<dyn Machine>> {
        self()
    }
}

struct Registration {
    descriptor: MachineDescriptor,
    factory: Option<Box<dyn MachineFactory>>,
}

#[derive(Default)]
pub struct MachineRegistry {
    machines: Vec<Registration>,
}

impl MachineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A missing factory advertises a planned machine without pretending it exists.
    pub fn register(
        &mut self,
        descriptor: MachineDescriptor,
        factory: Option<Box<dyn MachineFactory>>,
    ) -> Result<()> {
        if descriptor.id.is_empty()
            || self
                .machines
                .iter()
                .any(|m| m.descriptor.id == descriptor.id)
        {
            return Err(Error::InvalidInput(format!(
                "empty or duplicate machine id: {}",
                descriptor.id
            )));
        }
        self.machines.push(Registration {
            descriptor,
            factory,
        });
        Ok(())
    }

    pub fn machines(&self) -> impl Iterator<Item = (&MachineDescriptor, bool)> {
        self.machines
            .iter()
            .map(|entry| (&entry.descriptor, entry.factory.is_some()))
    }

    fn create(&self, id: &str) -> Result<Box<dyn Machine>> {
        let entry = self
            .machines
            .iter()
            .find(|entry| entry.descriptor.id == id)
            .ok_or_else(|| Error::InvalidInput(format!("unknown machine: {id}")))?;
        let factory = entry
            .factory
            .as_ref()
            .ok_or(Error::Unsupported("selected machine"))?;
        let machine = factory.create()?;
        if machine.info().id != id {
            return Err(Error::InvalidInput(format!(
                "factory for {id} created {}",
                machine.info().id
            )));
        }
        Ok(machine)
    }
}

pub struct MachineManager {
    registry: MachineRegistry,
    current: Option<Box<dyn Machine>>,
    states: StateManager,
}

impl MachineManager {
    pub fn new(registry: MachineRegistry, history_length: usize) -> Self {
        Self {
            registry,
            current: None,
            states: StateManager::new(history_length),
        }
    }

    pub fn registry(&self) -> &MachineRegistry {
        &self.registry
    }

    pub fn current(&self) -> Option<&dyn Machine> {
        self.current.as_deref()
    }

    fn active(&mut self) -> Result<&mut (dyn Machine + '_)> {
        match self.current.as_mut() {
            Some(machine) => Ok(machine.as_mut()),
            None => Err(Error::InvalidInput("no active machine".into())),
        }
    }

    /// Construct and validate the replacement before shutting down the old machine.
    pub fn load_machine(&mut self, id: &str) -> Result<()> {
        let replacement = self.registry.create(id)?;
        if let Some(machine) = &mut self.current {
            machine.shutdown()?;
        }
        self.current = Some(replacement);
        self.states = StateManager::new(self.states.history_length());
        Ok(())
    }

    pub fn switch_machine(&mut self, id: &str) -> Result<()> {
        self.load_machine(id)
    }

    pub fn restart_machine(&mut self) -> Result<()> {
        let id = self
            .current
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?
            .info()
            .id;
        self.load_machine(id)
    }

    pub fn unload_machine(&mut self) -> Result<()> {
        self.active()?.shutdown()?;
        self.current = None;
        self.states = StateManager::new(self.states.history_length());
        Ok(())
    }

    pub fn reset_machine(&mut self) -> Result<()> {
        self.active()?.reset()?;
        self.states.clear_history();
        Ok(())
    }

    pub fn set_history_length(&mut self, length: usize) {
        self.states.set_history_length(length);
    }

    pub fn input(&mut self, event: InputEvent) -> Result<()> {
        self.active()?.input(event)
    }

    pub fn run_frame(&mut self) -> Result<Frame<'_>> {
        let machine = self
            .current
            .as_mut()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?;
        self.states.run_frame(machine.as_mut())
    }

    pub fn load_media(&mut self, media: &Media<'_>) -> Result<()> {
        self.active()?.load_media(media)?;
        self.states.clear_history();
        Ok(())
    }

    pub fn load_media_path(&mut self, path: &Path) -> Result<()> {
        let format = path
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .ok_or_else(|| Error::InvalidInput("media path has no valid extension".into()))?
            .to_ascii_lowercase();
        let bytes = fs::read(path)?;
        self.load_media(&Media {
            format: &format,
            bytes: &bytes,
        })
    }

    pub fn eject_media(&mut self) -> Result<()> {
        self.active()?.eject_media()?;
        self.states.clear_history();
        Ok(())
    }

    pub fn media_information(&self) -> Result<Option<&'static str>> {
        self.current
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?
            .media_information()
    }

    pub fn save_state(&self) -> Result<State> {
        self.current
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?
            .save_state()
    }

    pub fn load_state(&mut self, state: &State) -> Result<()> {
        self.active()?.load_state(state)?;
        self.states.clear_history();
        Ok(())
    }

    pub fn quick_save(&mut self) -> Result<()> {
        let machine = self
            .current
            .as_ref()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?;
        self.states.quick_save(machine.as_ref())
    }

    pub fn quick_load(&mut self) -> Result<()> {
        let machine = self
            .current
            .as_mut()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?;
        self.states.quick_load(machine.as_mut())
    }

    pub fn rewind(&mut self) -> Result<()> {
        let machine = self
            .current
            .as_mut()
            .ok_or_else(|| Error::InvalidInput("no active machine".into()))?;
        self.states.rewind(machine.as_mut())
    }
}
