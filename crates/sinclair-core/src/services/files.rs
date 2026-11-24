use std::path::{Path, PathBuf};

use crate::{state::State, Result};

/// Cancellation is `Ok(None)`; an error must not be reported as cancellation.
pub trait FileBrowser {
    fn choose_file(&mut self) -> Result<Option<PathBuf>>;
}

/// The store owns envelope encoding and file I/O, not machine payload semantics.
pub trait StateStore {
    fn read(&self, path: &Path) -> Result<State>;
    fn write(&self, path: &Path, state: &State) -> Result<()>;
}
