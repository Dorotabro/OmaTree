//! An embeddable, source-backed Markdown editor for egui.
//!
//! [`EditorView`] renders live Markdown styling over one truly editable `egui::TextEdit` whose buffer
//! is the host's Markdown `String`. The host owns everything outside the text surface: the document
//! and where it is saved, the attachment store, the palette, the window, file dialogs and the
//! document-level shortcuts (Save, Open, Export dialogs). The library never reads or writes
//! documents, never inspects the environment and never opens windows.
//!
//! Exporters ([`export`], [`pdf`]) are plain functions over a Markdown string and an
//! [`attachments::AttachmentStore`], so any host can offer them.

pub use egui;

// Lets test code that is also compiled as an example name this crate as `omatree_editor`.
#[cfg(test)]
extern crate self as omatree_editor;

pub mod assets;
pub mod attachments;
pub mod edit;
pub mod emoji;
pub mod export;
pub mod fonts;
pub(crate) mod geom;
pub mod image_size;
pub mod keys;
pub mod layout;
pub mod links;
pub mod markdown;
pub mod pdf;
pub mod table;
#[cfg(test)]
mod test_dir;
pub mod theme;
pub mod view;

pub use keys::EditorKeys;
pub use view::{EditorInput, EditorOutput, EditorView, FocusRequest, NoteKey};
