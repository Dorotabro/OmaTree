//! Export of the selected note (ticket E11a): plain Markdown, a Markdown folder, a ZIP and a PDF.
//!
//! All four read the note's *current* text (the editor buffer, unsaved edits included) and its
//! images through [`DocStore`], so pending and committed attachments are both readable and nothing
//! is ever loaded from a file or URL. The formats themselves are the editor library's:
//! `export::plan` (what is referenced, validated), `export::write_dir`, `export::write_zip` and
//! `pdf::export_pdf`, each of which stages its output in a hidden sibling and renames it into place
//! only when complete and never replaces an existing destination. This module adds only what the
//! library does not have: the plain `.md` writer (its own `write_file` is crate-private), the
//! replace-after-confirmation step for single files (export to a fresh hidden sibling, then rename
//! over the old file, so a failed export never damages a valid existing one), and the dialogs.
//!
//! Exporting never changes the notebook: no edit, no checkpoint, no dirty-state change, no save,
//! nothing cleared (a conflict stays a conflict). The only thing done first is what every
//! document-level command does: the pending title and a pending image width are committed.

use super::lifecycle::{dialog_buttons, dialog_frame, dialog_heading, dialog_open};
use super::*;
use omatree_editor::{export, pdf};
use std::collections::BTreeSet;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Folder,
    Zip,
    Pdf,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [
        ExportFormat::Markdown,
        ExportFormat::Folder,
        ExportFormat::Zip,
        ExportFormat::Pdf,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Markdown => "Markdown file",
            ExportFormat::Folder => "Markdown folder with images",
            ExportFormat::Zip => "ZIP archive with images",
            ExportFormat::Pdf => "PDF document",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ExportFormat::Markdown => {
                "The note's text only, exactly as written. Images are not included."
            }
            ExportFormat::Folder => {
                "note.md and the images it uses in an assets folder, ready to move anywhere."
            }
            ExportFormat::Zip => "The same files in one archive: the easiest way to share a note.",
            ExportFormat::Pdf => "A formatted document with the images embedded.",
        }
    }

    /// The extension added when a destination has none (a folder has none).
    pub fn extension(self) -> Option<&'static str> {
        match self {
            ExportFormat::Markdown => Some("md"),
            ExportFormat::Folder => None,
            ExportFormat::Zip => Some("zip"),
            ExportFormat::Pdf => Some("pdf"),
        }
    }
}

/// A file name stem for `title`: characters no file system accepts everywhere become `_`.
pub(crate) fn suggest_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "note".to_owned()
    } else {
        // Well below the exporter's 200-byte limit, cut on a character boundary.
        trimmed.chars().take(80).collect()
    }
}

/// How many different notebook images `text` refers to (a plain-text count: no image is read).
fn image_count(text: &str) -> usize {
    let mut ids = BTreeSet::new();
    omatree_core::attachments::scan_references(text, |id| {
        ids.insert(id.to_owned());
    });
    ids.len()
}

/// A hidden sibling of `dest` that does not exist yet.
fn sibling(dest: &Path, tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = dest
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    dest.with_file_name(format!(
        ".{name}.{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

/// Publish `bytes` as `dest`: staged in a hidden sibling, synced, renamed into place. Without
/// `replace` an existing destination is an error; with it only a regular file is replaced, and a
/// failure leaves the old file untouched and nothing partial behind.
pub(crate) fn write_file(bytes: &[u8], dest: &Path, replace: bool) -> Result<(), String> {
    let tmp = sibling(dest, "partial");
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("cannot create {}: {e}", tmp.display()))?;
        f.write_all(bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
        publish(&tmp, dest, replace)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Move the finished `tmp` onto `dest`.
fn publish(tmp: &Path, dest: &Path, replace: bool) -> Result<(), String> {
    match std::fs::symlink_metadata(dest) {
        Ok(_) if !replace => {
            return Err(format!("{} already exists", dest.display()));
        }
        Ok(m) if !m.is_file() => {
            return Err(format!(
                "{} is not a file and is not replaced",
                dest.display()
            ));
        }
        _ => {}
    }
    std::fs::rename(tmp, dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))
}

/// What a successful export produced, for the status line.
struct Done {
    images: usize,
    pages: Option<usize>,
}

impl Host {
    // --- Opening -----------------------------------------------------------------------------

    /// File ▸ Export note… and Ctrl+Shift+E. Nothing is written until a destination is chosen.
    pub fn start_export(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || self.dialog_open() {
            return;
        }
        if self.selected.is_none() {
            self.status = "Select a note to export.".to_owned();
            return;
        }
        // The same settling every document-level command does: a pending title and a pending
        // image width are committed, and the editor's text reaches the document.
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        let images = image_count(&self.text);
        let format = if images > 0 {
            ExportFormat::Zip
        } else {
            ExportFormat::Markdown
        };
        self.open_dialog(ctx, Dialog::Export { format, images });
    }

    pub fn set_export_format(&mut self, format: ExportFormat) {
        if let Dialog::Export { format: f, .. } = &mut self.dialog {
            *f = format;
        }
    }

    fn export_title(&self) -> String {
        self.selected
            .and_then(|id| self.doc.as_ref()?.notebook().get(id))
            .map_or_else(String::new, |n| n.title().to_owned())
    }

    fn export_dir(&self) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    fn export_file_name(&self, format: ExportFormat) -> String {
        let stem = suggest_stem(&self.export_title());
        match format.extension() {
            Some(ext) => format!("{stem}.{ext}"),
            None => stem,
        }
    }

    /// "Choose destination…": the native dialog when there is one, else the typed path.
    pub fn choose_export_destination(&mut self, ctx: &egui::Context) {
        let Dialog::Export { format, .. } = self.dialog.clone() else {
            return;
        };
        if self.picker.is_some() {
            if self.pick.is_some() {
                return;
            }
            let req = PickRequest {
                kind: PickKind::Export(format),
                dir: Some(self.export_dir()),
                name: self.export_file_name(format),
            };
            self.begin_pick(ctx, req);
            return;
        }
        self.open_typed_export(ctx, format);
    }

    /// Export by typing the destination (no native dialog, or the user chose to type).
    pub(crate) fn open_typed_export(&mut self, ctx: &egui::Context, format: ExportFormat) {
        let suggested = self.export_dir().join(self.export_file_name(format));
        self.open_dialog(
            ctx,
            Dialog::ExportPath {
                format,
                path: suggested.display().to_string(),
                error: None,
            },
        );
    }

    pub fn set_export_path(&mut self, text: &str) {
        if let Dialog::ExportPath { path, .. } = &mut self.dialog {
            *path = text.to_owned();
        }
    }

    // --- The destination ---------------------------------------------------------------------

    /// The destination a typed (or chosen) path means: relative paths are taken from the
    /// notebook's folder, and a missing extension is added (never for a folder).
    fn export_target(&self, format: ExportFormat, typed: &str) -> Option<PathBuf> {
        let typed = typed.trim();
        if typed.is_empty() {
            return None;
        }
        let mut p = PathBuf::from(typed);
        if let (Some(ext), None) = (format.extension(), p.extension()) {
            p.set_extension(ext);
        }
        // The desktop portal's file chooser appends the filter's extension to a name that
        // already has it (`note.zip` comes back as `note.zip.zip`, observed with the GTK portal).
        // A doubled extension is never what anyone means.
        if let (Some(ext), Some(name)) =
            (format.extension(), p.file_name().and_then(|n| n.to_str()))
        {
            let lower = name.to_lowercase();
            let doubled = format!(".{ext}.{ext}");
            if lower.ends_with(&doubled) {
                let keep = name.len() - ext.len() - 1;
                let fixed = name[..keep].to_owned();
                p.set_file_name(fixed);
            }
        }
        Some(if p.is_absolute() {
            p
        } else {
            self.export_dir().join(p)
        })
    }

    /// `export_target`, for tests.
    #[cfg(test)]
    pub(crate) fn export_target_for_test(
        &self,
        format: ExportFormat,
        typed: &str,
    ) -> Option<PathBuf> {
        self.export_target(format, typed)
    }

    /// A native folder dialog answers with an existing parent; the export folder is made inside.
    pub(crate) fn submit_picked_export(
        &mut self,
        ctx: &egui::Context,
        format: ExportFormat,
        picked: &Path,
    ) {
        let target = if format == ExportFormat::Folder {
            picked.join(suggest_stem(&self.export_title()))
        } else {
            picked.to_path_buf()
        };
        self.submit_export_target(ctx, format, &target.to_string_lossy());
    }

    /// Enter in the typed dialog.
    pub fn submit_export_path(&mut self, ctx: &egui::Context) {
        let Dialog::ExportPath { format, path, .. } = self.dialog.clone() else {
            return;
        };
        self.submit_export_target(ctx, format, &path);
    }

    pub(crate) fn submit_export_target(
        &mut self,
        ctx: &egui::Context,
        format: ExportFormat,
        typed: &str,
    ) {
        let again = |host: &mut Host, error: String| {
            host.set_dialog(Dialog::ExportPath {
                format,
                path: typed.to_owned(),
                error: Some(error),
            });
        };
        let Some(target) = self.export_target(format, typed) else {
            return again(self, "Enter a file name.".to_owned());
        };
        if self.is_notebook_file(&target) {
            return again(
                self,
                "That is the notebook's own file. Choose another destination.".to_owned(),
            );
        }
        match std::fs::symlink_metadata(&target) {
            Ok(_) if format == ExportFormat::Folder => again(
                self,
                format!(
                    "\u{201c}{}\u{201d} already exists. A folder is never replaced: choose another name.",
                    file_name(&target)
                ),
            ),
            Ok(_) => self.set_dialog(Dialog::ExportOverwrite {
                format,
                path: target,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.run_export(ctx, format, target, false);
            }
            Err(e) => again(self, format!("Cannot use that destination: {e}")),
        }
    }

    pub fn confirm_export_overwrite(&mut self, ctx: &egui::Context) {
        if let Dialog::ExportOverwrite { format, path } = self.dialog.clone() {
            self.run_export(ctx, format, path, true);
        }
    }

    /// Whether `target` is the open notebook's file (or its journal), however it is spelled.
    fn is_notebook_file(&self, target: &Path) -> bool {
        let Some(nb) = self.path.as_deref() else {
            return false;
        };
        let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x == y,
            _ => a == b,
        };
        let mut journal = nb.as_os_str().to_owned();
        journal.push("-journal");
        same(target, nb) || same(target, Path::new(&journal))
    }

    // --- Writing -----------------------------------------------------------------------------

    /// Write the export. The notebook, the note and its dirty state are only read.
    fn run_export(
        &mut self,
        _ctx: &egui::Context,
        format: ExportFormat,
        dest: PathBuf,
        replace: bool,
    ) {
        let title = self.export_title();
        let result = self.write_export(format, &title, &dest, replace);
        // Whatever the adapter noted about unreadable images belongs to this export, not to the
        // next frame's status line.
        let read_problem = self.images.take_read();
        let _ = self.images.take_put();
        match result {
            Ok(done) => {
                self.status = match (format, done.pages) {
                    (ExportFormat::Markdown, _) => {
                        let n = image_count(&self.text);
                        if n > 0 {
                            format!(
                                "Exported {} (text only: {n} notebook image{} not included; use a folder or ZIP to keep them)",
                                file_name(&dest),
                                if n == 1 { " is" } else { "s are" }
                            )
                        } else {
                            format!("Exported {}", file_name(&dest))
                        }
                    }
                    (ExportFormat::Pdf, Some(p)) => format!(
                        "Exported {} ({p} page{}, {} image{})",
                        file_name(&dest),
                        if p == 1 { "" } else { "s" },
                        done.images,
                        if done.images == 1 { "" } else { "s" }
                    ),
                    _ => format!(
                        "Exported {} ({} image{})",
                        file_name(&dest),
                        done.images,
                        if done.images == 1 { "" } else { "s" }
                    ),
                };
                self.close_dialog();
            }
            Err(mut e) => {
                if let Some(r) = read_problem {
                    // The library reports an unreadable image generically; say which kind.
                    e = format!("{e} ({})", describe_read_problem(&r));
                }
                if e.contains("No space left") {
                    e = format!("The destination is full: {e}");
                }
                let message = format!("Export failed: {e}");
                self.status = message.clone();
                self.set_dialog(Dialog::ExportPath {
                    format,
                    path: dest.display().to_string(),
                    error: Some(message),
                });
            }
        }
    }

    fn write_export(
        &self,
        format: ExportFormat,
        title: &str,
        dest: &Path,
        replace: bool,
    ) -> Result<Done, String> {
        let doc = self.doc.as_ref().ok_or("no notebook is open")?;
        let store = DocStore::new(doc, &self.images);
        let text = self.text.as_str();
        let name = dest
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let stringify = |e: export::ExportError| e.to_string();
        // For a single file that is replaced, build it at a fresh sibling first.
        let target = if replace && format != ExportFormat::Folder {
            sibling(dest, "replace")
        } else {
            dest.to_path_buf()
        };
        let built = (|| -> Result<Done, String> {
            match format {
                ExportFormat::Markdown => {
                    // Validates the name the way the other formats do, but does not require the
                    // images to exist: the file carries none of them.
                    export::note_name(&name).map_err(stringify)?;
                    write_file(text.as_bytes(), &target, false)?;
                    Ok(Done {
                        images: 0,
                        pages: None,
                    })
                }
                ExportFormat::Folder | ExportFormat::Zip => {
                    let md_name = suggest_stem(title);
                    let plan = export::plan(text, &store, &md_name).map_err(stringify)?;
                    let images = plan.attachments.len();
                    if format == ExportFormat::Folder {
                        export::write_dir(&plan, &target).map_err(stringify)?;
                    } else {
                        export::write_zip(&plan, &target).map_err(stringify)?;
                    }
                    Ok(Done {
                        images,
                        pages: None,
                    })
                }
                ExportFormat::Pdf => {
                    let s = pdf::export_pdf(text, &store, title, &target).map_err(stringify)?;
                    Ok(Done {
                        images: s.images,
                        pages: Some(s.pages),
                    })
                }
            }
        })();
        if target != dest {
            match built {
                Ok(done) => {
                    return match publish(&target, dest, true) {
                        Ok(()) => Ok(done),
                        Err(e) => {
                            let _ = std::fs::remove_file(&target);
                            Err(e)
                        }
                    };
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&target);
                    return Err(e);
                }
            }
        }
        built
    }

    // --- The dialogs -------------------------------------------------------------------------

    pub(crate) fn show_export(&mut self, ctx: &egui::Context, first_frame: bool) {
        let Dialog::Export { format, images } = self.dialog.clone() else {
            return;
        };
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let title = self.export_title();
        let mut chosen = format;
        let (mut go, mut typed, mut cancel) = (false, false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-export"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 460.0);
                dialog_heading(ui, "Export note", bold.as_ref(), &ch);
                ui.label(format!("\u{201c}{title}\u{201d}"));
                // Up and Down move through the formats; Enter chooses the destination.
                let (up, down, enter) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    )
                });
                let at = ExportFormat::ALL
                    .iter()
                    .position(|f| *f == chosen)
                    .unwrap_or(0);
                if up && at > 0 {
                    chosen = ExportFormat::ALL[at - 1];
                }
                if down && at + 1 < ExportFormat::ALL.len() {
                    chosen = ExportFormat::ALL[at + 1];
                }
                for f in ExportFormat::ALL {
                    let r = ui.radio_value(&mut chosen, f, f.label());
                    if first_frame && f == format {
                        r.request_focus();
                    }
                    ui.indent("export-desc", |ui| {
                        ui.label(
                            egui::RichText::new(f.description())
                                .size(12.5)
                                .color(ch.muted),
                        );
                    });
                }
                if images > 0 && chosen == ExportFormat::Markdown {
                    ui.label(
                        egui::RichText::new(format!(
                            "This note shows {images} notebook image{}. A Markdown file does not \
                             contain them: choose the folder or ZIP to keep them.",
                            if images == 1 { "" } else { "s" }
                        ))
                        .color(ch.warning_text),
                    );
                }
                dialog_buttons(ui, |ui| {
                    go = ui.button("Choose destination\u{2026}").clicked() || enter;
                    typed = ui.button("Type a path\u{2026}").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if let Dialog::Export { format: f, .. } = &mut self.dialog {
            *f = chosen;
        }
        if cancel || m.should_close() {
            self.cancel_dialog();
        } else if typed {
            self.open_typed_export(ctx, chosen);
        } else if go {
            self.choose_export_destination(ctx);
        }
    }

    pub(crate) fn show_export_path(&mut self, ctx: &egui::Context, first_frame: bool) {
        let Dialog::ExportPath {
            format,
            path,
            error,
        } = self.dialog.clone()
        else {
            return;
        };
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let mut typed = path;
        let (mut submit, mut cancel) = (false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-export-path"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 480.0);
                dialog_heading(ui, "Export note", bold.as_ref(), &ch);
                ui.label(match format {
                    ExportFormat::Folder => "Path of the new folder:",
                    _ => "Path of the new file:",
                });
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut typed)
                        .desired_width(f32::INFINITY)
                        .id(egui::Id::unique("dlg-export-path-field")),
                );
                if first_frame {
                    edit.request_focus();
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(e) = &error {
                    ui.label(egui::RichText::new(e).color(ch.danger_text));
                }
                dialog_buttons(ui, |ui| {
                    if ui.button("Export").clicked() || enter {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        self.set_export_path(&typed);
        if cancel || m.should_close() {
            self.cancel_dialog();
        } else if submit {
            self.submit_export_path(ctx);
        }
    }

    pub(crate) fn show_export_overwrite(&mut self, ctx: &egui::Context, first_frame: bool) {
        let Dialog::ExportOverwrite { path, .. } = self.dialog.clone() else {
            return;
        };
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let (mut yes, mut cancel) = (false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-export-overwrite"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 360.0);
                dialog_heading(ui, "Replace file", bold.as_ref(), &ch);
                ui.label(format!(
                    "\u{201c}{}\u{201d} already exists. Replace it with this export?",
                    file_name(&path)
                ));
                dialog_buttons(ui, |ui| {
                    let no = ui.button("Cancel");
                    if first_frame {
                        no.request_focus();
                    }
                    cancel = no.clicked();
                    yes = lifecycle::destructive_button(ui, "Yes, replace", &ch).clicked();
                });
            });
        if cancel || m.should_close() {
            self.cancel_dialog();
        } else if yes {
            self.confirm_export_overwrite(ctx);
        }
    }
}

fn describe_read_problem(e: &omatree_core::attachments::AttachmentError) -> String {
    use omatree_core::attachments::AttachmentError as E;
    match e {
        E::Corrupt(why) => format!("a stored image is damaged: {why}"),
        other => format!("an image could not be read: {other}"),
    }
}
