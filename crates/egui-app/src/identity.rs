//! Which notebook is open, and where it is saved (M1).
//!
//! The window title already carries the file name; the status bar shows it quietly too, with the
//! full location in a tooltip, and the notebook menu opens with both so that the keyboard reaches
//! it as well. Nothing here is stored: the name is the open `Document`'s own and the location is
//! the path the host already keeps for Save As and Reload, made absolute only for display. Paths
//! are shown as they are (no normalisation of Unicode, spaces or separators).

use super::*;

/// What the interface says about the open notebook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The file name, or `Untitled` before the first save.
    pub name: String,
    /// The absolute path, or `None` while the notebook has no file.
    pub location: Option<String>,
}

impl Identity {
    /// The tooltip: the full path, or what saving will do for a notebook with no file yet.
    pub fn tooltip(&self, save_shortcut: &str) -> String {
        match &self.location {
            Some(path) => path.clone(),
            None => format!(
                "This notebook has not been saved yet. {save_shortcut} asks where to put it."
            ),
        }
    }

    /// The two lines at the top of the notebook menu: the name, and the folder (or that it is not
    /// saved yet).
    pub fn menu_lines(&self) -> (String, Option<String>) {
        let folder = match &self.location {
            Some(path) => Path::new(path)
                .parent()
                .map(|p| p.display().to_string())
                .filter(|p| !p.is_empty()),
            None => Some("not saved yet".to_owned()),
        };
        (self.name.clone(), folder)
    }
}

/// `path` as an absolute path for display (a path given relative to the working directory at start,
/// say). Only the spelling of the directory is completed; the path itself is not rewritten.
pub(crate) fn absolute_for_display(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

impl Host {
    /// The open notebook's name and location, or `None` when no notebook is open.
    pub fn identity(&self) -> Option<Identity> {
        let doc = self.doc.as_ref()?;
        Some(Identity {
            name: doc.display_name(),
            location: doc
                .has_path()
                .then(|| self.path.as_deref().map(absolute_for_display))
                .flatten(),
        })
    }
}
