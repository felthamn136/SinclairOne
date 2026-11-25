use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use sinclair_core::{
    manager::{MachineDescriptor, MachineFactory, MachineManager, MachineRegistry},
    Error, Machine, Result,
};
use sinclair_spectrum128::Spectrum128;
use sinclair_spectrum48::Spectrum48;
use sinclair_zx81::{memory::RamSize, Zx81};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MachineConfig {
    pub startup_machine: Option<String>,
    pub zx81_rom: Option<PathBuf>,
    pub zx81_ram_kib: u8,
    pub spectrum48_rom: Option<PathBuf>,
    pub spectrum128_rom0: Option<PathBuf>,
    pub spectrum128_rom1: Option<PathBuf>,
    pub rewind_history_length: usize,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            startup_machine: None,
            zx81_rom: None,
            zx81_ram_kib: 1,
            spectrum48_rom: None,
            spectrum128_rom0: None,
            spectrum128_rom1: None,
            rewind_history_length: 300,
        }
    }
}

impl MachineConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.into()),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|error| Error::InvalidInput(format!("machine configuration: {error}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if !matches!(self.zx81_ram_kib, 1 | 16) {
            return Err(Error::InvalidInput("ZX81 RAM must be 1 or 16 KiB".into()));
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| Error::InvalidInput(format!("machine configuration: {error}")))?;
        let name = path
            .file_name()
            .ok_or_else(|| Error::InvalidInput("configuration path has no file name".into()))?;
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = path.with_file_name(temporary_name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let write_result = file.write_all(&bytes).and_then(|_| file.sync_all());
        drop(file);
        let result = write_result.and_then(|_| fs::rename(&temporary, path));
        if let Err(error) = result {
            if let Err(cleanup) = fs::remove_file(&temporary) {
                return Err(Error::Io(std::io::Error::new(
                    error.kind(),
                    format!("{error}; could not remove temporary configuration: {cleanup}"),
                )));
            }
            return Err(error.into());
        }
        Ok(())
    }
}

fn rom(path: &Option<PathBuf>, label: &str) -> Result<Vec<u8>> {
    let path = path
        .as_ref()
        .ok_or_else(|| Error::InvalidInput(format!("configure {label} in machine_config.json")))?;
    fs::read(path).map_err(|error| {
        Error::Io(std::io::Error::new(
            error.kind(),
            format!("reading {label} {}: {error}", path.display()),
        ))
    })
}

fn descriptor(id: &str, name: &str, year: u16, cpu: &str, memory: &str) -> MachineDescriptor {
    MachineDescriptor {
        id: id.into(),
        name: name.into(),
        manufacturer: "Sinclair Research".into(),
        year,
        cpu: cpu.into(),
        memory: memory.into(),
    }
}

pub fn select_machine(
    manager: &mut MachineManager,
    config: &mut MachineConfig,
    config_path: &Path,
    id: &str,
    persist: bool,
) -> Result<()> {
    manager.load_machine(id)?;
    if persist {
        let mut next = config.clone();
        next.startup_machine = Some(id.into());
        next.save(config_path)?;
        *config = next;
    }
    Ok(())
}

pub fn registry(config: &MachineConfig) -> Result<MachineRegistry> {
    config.validate()?;
    let mut registry = MachineRegistry::new();
    let zx81 = config.clone();
    let zx81_factory: Box<dyn MachineFactory> = Box::new(move || -> Result<Box<dyn Machine>> {
        let ram = match zx81.zx81_ram_kib {
            1 => RamSize::OneK,
            16 => RamSize::SixteenK,
            _ => return Err(Error::InvalidInput("ZX81 RAM must be 1 or 16 KiB".into())),
        };
        Ok(Box::new(Zx81::new(&rom(&zx81.zx81_rom, "zx81_rom")?, ram)?))
    });
    registry.register(
        descriptor("zx81", "Sinclair ZX81", 1981, "Z80", "1K/16K"),
        Some(zx81_factory),
    )?;
    registry.register(
        descriptor("spectrum16", "ZX Spectrum 16K", 1982, "Z80", "16K"),
        None,
    )?;
    let spectrum48 = config.clone();
    let spectrum48_factory: Box<dyn MachineFactory> =
        Box::new(move || -> Result<Box<dyn Machine>> {
            Ok(Box::new(Spectrum48::new(&rom(
                &spectrum48.spectrum48_rom,
                "spectrum48_rom",
            )?)?))
        });
    registry.register(
        descriptor("spectrum48", "ZX Spectrum 48K", 1982, "Z80", "48K"),
        Some(spectrum48_factory),
    )?;
    let spectrum128 = config.clone();
    let spectrum128_factory: Box<dyn MachineFactory> =
        Box::new(move || -> Result<Box<dyn Machine>> {
            Ok(Box::new(Spectrum128::new(
                &rom(&spectrum128.spectrum128_rom0, "spectrum128_rom0")?,
                &rom(&spectrum128.spectrum128_rom1, "spectrum128_rom1")?,
            )?))
        });
    registry.register(
        descriptor("spectrum128", "ZX Spectrum 128K", 1985, "Z80", "128K"),
        Some(spectrum128_factory),
    )?;
    registry.register(descriptor("ql", "Sinclair QL", 1984, "68008", "128K"), None)?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sinclair_core::media::Media;

    #[test]
    fn configuration_round_trips_and_rejects_invalid_values() {
        let path = std::env::temp_dir().join(format!(
            "sinclair-machine-config-{}.json",
            std::process::id()
        ));
        let config = MachineConfig {
            startup_machine: Some("zx81".into()),
            zx81_ram_kib: 16,
            ..MachineConfig::default()
        };
        config.save(&path).unwrap();
        assert_eq!(MachineConfig::load(&path).unwrap(), config);
        let updated = MachineConfig {
            startup_machine: Some("spectrum128".into()),
            ..config
        };
        updated.save(&path).unwrap();
        assert_eq!(MachineConfig::load(&path).unwrap(), updated);
        fs::write(&path, br#"{"zx81_ram_kib":2}"#).unwrap();
        assert!(MachineConfig::load(&path).is_err());
        fs::write(&path, br#"{"unrecognised":true}"#).unwrap();
        assert!(MachineConfig::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn available_factories_load_roms_and_planned_machines_are_rejected() {
        let root = std::env::temp_dir();
        let zx = root.join(format!("sinclair-zx81-{}.rom", std::process::id()));
        let spectrum = root.join(format!("sinclair-spectrum-{}.rom", std::process::id()));
        fs::write(&zx, vec![0; 8192]).unwrap();
        fs::write(&spectrum, vec![0; 16384]).unwrap();
        let mut config = MachineConfig {
            zx81_rom: Some(zx.clone()),
            zx81_ram_kib: 16,
            spectrum48_rom: Some(spectrum.clone()),
            spectrum128_rom0: Some(spectrum.clone()),
            spectrum128_rom1: Some(spectrum.clone()),
            ..MachineConfig::default()
        };
        let mut manager = MachineManager::new(registry(&config).unwrap(), 2);
        for id in ["zx81", "spectrum48", "spectrum128"] {
            let start = std::time::Instant::now();
            manager.load_machine(id).unwrap();
            assert!(
                start.elapsed() < std::time::Duration::from_secs(1),
                "switch to {id} exceeded one second with local test ROMs"
            );
            manager.quick_save().unwrap();
            manager.quick_load().unwrap();
        }
        manager.load_machine("spectrum48").unwrap();
        manager
            .load_media(&Media {
                format: "tap",
                bytes: &[2, 0, 0xff, 0xff],
            })
            .unwrap();
        assert_eq!(manager.media_information().unwrap(), Some("tap"));
        let state = manager.save_state().unwrap();
        manager.eject_media().unwrap();
        assert_eq!(manager.media_information().unwrap(), None);
        manager.load_state(&state).unwrap();
        assert_eq!(manager.media_information().unwrap(), Some("tap"));
        manager.load_machine("spectrum128").unwrap();
        let config_path = root.join(format!("sinclair-selection-{}.json", std::process::id()));
        select_machine(&mut manager, &mut config, &config_path, "zx81", true).unwrap();
        assert_eq!(
            MachineConfig::load(&config_path)
                .unwrap()
                .startup_machine
                .as_deref(),
            Some("zx81")
        );
        assert!(
            select_machine(&mut manager, &mut config, &config_path, "spectrum16", true).is_err()
        );
        assert_eq!(
            MachineConfig::load(&config_path)
                .unwrap()
                .startup_machine
                .as_deref(),
            Some("zx81")
        );
        assert!(manager.load_machine("ql").is_err());
        assert_eq!(manager.current().unwrap().info().id, "zx81");
        fs::remove_file(config_path).unwrap();
        fs::remove_file(zx).unwrap();
        fs::remove_file(spectrum).unwrap();
    }
}
