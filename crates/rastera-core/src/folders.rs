use std::path::{Path, PathBuf};

use crate::{CancellationToken, ErrorKind, FileStamp, RasteraError, Result};

pub const MAX_FOLDER_ENTRIES: usize = 10_000;
pub const MAX_FOLDER_PATH_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub path: PathBuf,
    pub stamp: Option<FileStamp>,
}

/// Coalesced hints, never an authoritative list of filesystem operations.
#[derive(Default)]
pub struct FolderChanges {
    pub paths: Vec<PathBuf>,
    pub rescan: bool,
}

pub trait FolderWatch: Send {
    fn take_changes(&self) -> Result<FolderChanges>;
}

pub trait FolderBackend: Send + Sync + 'static {
    fn watch(&self, _path: &Path) -> Result<Box<dyn FolderWatch>> {
        Err(RasteraError::new(
            ErrorKind::Unsupported,
            "Observador no disponible",
        ))
    }
    fn is_directory(&self, path: &Path) -> Result<bool>;
    fn scan(
        &self,
        path: &Path,
        cancel: &CancellationToken,
        emit: &mut dyn FnMut(Vec<FolderEntry>) -> Result<()>,
    ) -> Result<()>;
}
