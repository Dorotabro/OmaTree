//! OmaTree's domain core: the notebook tree, the open document and its
//! lifecycle, SQLite persistence, recovery (Trash and checkpoints), search, the
//! semantic theme palette and link safety.
//!
//! Pure Rust. This crate knows nothing about any UI toolkit (no Qt, no egui);
//! the applications that show a notebook depend on it.

pub mod attachments;
pub mod document;
pub mod link;
pub mod notebook;
pub mod recovery;
pub mod search;
pub mod storage;
pub mod theme;

#[cfg(test)]
mod attachment_tests;
#[cfg(test)]
mod compact_tests;
// Unix only, like the E9a/E9b helpers it builds on: permissions, `unshare`, `ulimit`, SIGKILL.
#[cfg(all(test, unix))]
mod integrity_tests;
