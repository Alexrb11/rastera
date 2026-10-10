//! Filesystem and image codec implementations of the domain contracts.

mod images;
pub use images::LocalImages;

mod folders;
pub use folders::LocalFolders;
mod catalog;
pub use catalog::{SqliteCatalog, catalog_path};
mod batch;
pub use batch::LocalBatch;
mod thumbnails;
pub use thumbnails::CachedThumbnails;
mod recovery;
