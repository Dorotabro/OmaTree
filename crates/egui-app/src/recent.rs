//! The list of recently opened notebooks (ticket E8b): file paths and their order, nothing else.
//!
//! It lives in a plain text file in the per-user configuration folder
//! (`$XDG_CONFIG_HOME/omatree/recent.txt`, else `~/.config/omatree/recent.txt`; `%APPDATA%` on
//! Windows, `~/Library/Application Support` on macOS are the conventional places but only the
//! XDG one has been exercised), one path per line, newest first. It is never part of a notebook
//! file. Everything about it is best-effort: a missing, unreadable, malformed or read-only file
//! gives an empty list or a list that is simply not saved, never an error in front of the user.
//! Paths are only believed when they are opened.

use std::path::{Path, PathBuf};

const MAX: usize = 8;

#[derive(Default)]
pub struct Recent {
    file: Option<PathBuf>,
    items: Vec<PathBuf>,
}

/// Where the list is kept, or `None` when no home or configuration folder can be found.
pub fn default_file() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(std::env::var_os("HOME").filter(|v| !v.is_empty())?).join(".config"),
    };
    Some(base.join("omatree").join("recent.txt"))
}

impl Recent {
    /// The list stored in `file` (empty if it cannot be read); `None` keeps it in memory only.
    pub fn load(file: Option<PathBuf>) -> Self {
        let items = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|text| {
                text.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .take(MAX)
                    .collect()
            })
            .unwrap_or_default();
        Recent { file, items }
    }

    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }

    /// `path` was opened (or saved to): it becomes the newest entry.
    pub fn push(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.items.retain(|p| p != &path);
        self.items.insert(0, path);
        self.items.truncate(MAX);
        self.save();
    }

    pub fn remove(&mut self, path: &Path) {
        let before = self.items.len();
        self.items.retain(|p| p != path);
        if self.items.len() != before {
            self.save();
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        let text: String = self
            .items
            .iter()
            .filter_map(|p| p.to_str())
            .filter(|p| !p.contains('\n'))
            .map(|p| format!("{p}\n"))
            .collect();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Best effort: a read-only or missing configuration folder just means it is not kept.
        let _ = std::fs::write(file, text);
    }
}
