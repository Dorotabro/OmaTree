//! The list of recently opened notebooks (ticket E8b): file paths and their order, nothing else.
//!
//! It lives in a plain text file in the per-user configuration folder
//! (`$XDG_CONFIG_HOME/omatree/recent.txt`, else `%APPDATA%\omatree\recent.txt` on Windows,
//! `~/Library/Application Support/omatree/recent.txt` on macOS and `~/.config/omatree/recent.txt`
//! elsewhere; E12c added the Windows and macOS rules, whose paths are unit-tested but whose run-time
//! behaviour is unverified), one path per line, newest first. It is never part of a notebook
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

/// The platforms whose conventional per-user configuration folder differs (E12c).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    pub(crate) const CURRENT: Platform = if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Other
    };
}

/// The per-user configuration folder for `platform`, given an environment lookup. An explicit
/// `XDG_CONFIG_HOME` wins everywhere (it is how tests and isolated runs redirect the file); then
/// `%APPDATA%` on Windows, `~/Library/Application Support` on macOS and `~/.config` elsewhere.
/// Before E12c only the XDG and `HOME` rules existed, so on Windows (no `HOME`) the list was
/// silently never saved. Windows and macOS are unverified at run time.
pub(crate) fn config_base(
    platform: Platform,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let set = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(x) = set("XDG_CONFIG_HOME") {
        return Some(x);
    }
    match platform {
        Platform::Windows => set("APPDATA"),
        Platform::MacOs => set("HOME").map(|h| h.join("Library").join("Application Support")),
        Platform::Other => set("HOME").map(|h| h.join(".config")),
    }
}

/// Where the list is kept, or `None` when no home or configuration folder can be found.
pub fn default_file() -> Option<PathBuf> {
    Some(
        config_base(Platform::CURRENT, |k| std::env::var_os(k))?
            .join("omatree")
            .join("recent.txt"),
    )
}

/// `\\?\C:\dir\file` (what `canonicalize` returns on Windows) without its `\\?\` prefix, so a
/// remembered path reads and compares like the one the user chose. UNC (`\\?\UNC\…`) and other
/// verbatim forms are left alone.
pub(crate) fn without_verbatim_prefix(path: &str) -> &str {
    match path.strip_prefix(r"\\?\") {
        Some(rest)
            if rest.as_bytes().get(1) == Some(&b':')
                && rest.as_bytes()[0].is_ascii_alphabetic() =>
        {
            rest
        }
        _ => path,
    }
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
        let path = match path.to_str().map(without_verbatim_prefix) {
            Some(plain) if cfg!(windows) => PathBuf::from(plain),
            _ => path,
        };
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
