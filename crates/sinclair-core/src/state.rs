use crate::{Error, Result};

/// An in-memory envelope; persistent encoding is a host concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub machine_id: String,
    pub version: u32,
    pub payload: Vec<u8>,
}

impl State {
    pub fn validate_for(&self, machine_id: &str, version: u32) -> Result<()> {
        if self.machine_id != machine_id || self.version != version {
            return Err(Error::IncompatibleState(format!(
                "expected {machine_id} version {version}, got {} version {}",
                self.machine_id, self.version
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_requires_matching_machine_and_version() {
        let state = State {
            machine_id: "spectrum48".into(),
            version: 1,
            payload: vec![1],
        };
        assert!(state.validate_for("spectrum48", 1).is_ok());
        assert!(matches!(
            state.validate_for("zx81", 1),
            Err(Error::IncompatibleState(_))
        ));
        assert!(matches!(
            state.validate_for("spectrum48", 2),
            Err(Error::IncompatibleState(_))
        ));
    }
}
