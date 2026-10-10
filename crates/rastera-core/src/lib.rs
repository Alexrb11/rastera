//! Shared domain types and contracts, independent of UI and persistence.

/// Application name shared by the desktop entry point and diagnostics.
pub const APP_NAME: &str = "Rastera";

mod images;
pub use images::*;

mod folders;
pub use folders::*;
mod catalog;
pub use catalog::*;
