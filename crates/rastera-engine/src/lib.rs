//! Application workflows and resource scheduling.

mod catalog;
mod loader;
pub mod memory;
pub use catalog::{CatalogCommand, CatalogEvent, CatalogWorker};
pub use loader::{ImageLoader, LoadedImage};

mod folders;
pub use folders::{FolderEvent, FolderScanner, FolderUpdate};
