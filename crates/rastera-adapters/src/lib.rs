//! Filesystem and image codec implementations of the domain contracts.

mod images;
pub use images::LocalImages;

mod folders;
pub use folders::LocalFolders;
mod catalog;
pub use catalog::{SqliteCatalog, catalog_path};
