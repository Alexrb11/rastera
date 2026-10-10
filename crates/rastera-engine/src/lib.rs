//! Application workflows and resource scheduling.

mod loader;
pub mod memory;
pub use loader::{ImageLoader, LoadedImage};

mod folders;
pub use folders::{FolderEvent, FolderScanner, FolderUpdate};
