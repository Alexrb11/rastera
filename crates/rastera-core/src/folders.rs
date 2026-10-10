use std::path::{Path, PathBuf};

use crate::{CancellationToken, Result};

pub const MAX_FOLDER_ENTRIES: usize = 10_000;
pub const MAX_FOLDER_PATH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct FolderEntry {
    pub path: PathBuf,
}

pub trait FolderBackend: Send + Sync + 'static {
    fn is_directory(&self, path: &Path) -> Result<bool>;
    fn scan(
        &self,
        path: &Path,
        cancel: &CancellationToken,
        emit: &mut dyn FnMut(Vec<FolderEntry>) -> Result<()>,
    ) -> Result<()>;
}
