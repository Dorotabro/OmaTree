//! OmaTree's domain core: the notebook tree, the open document and its
//! lifecycle, SQLite persistence, recovery (Trash and checkpoints), search, the
//! semantic theme palette and link safety.
//!
//! Pure Rust. This crate knows nothing about any UI toolkit (no Qt, no egui);
//! the applications that show a notebook depend on it.

pub mod document;
pub mod link;
pub mod notebook;
pub mod recovery;
pub mod search;
pub mod storage;
pub mod theme;
