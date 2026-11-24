use crate::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub audio_enabled: bool,
    pub crt_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio_enabled: true,
            crt_enabled: false,
        }
    }
}

pub trait ConfigStore {
    fn load(&self) -> Result<Config>;
    fn save(&self, config: &Config) -> Result<()>;
}
