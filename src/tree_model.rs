//! Qt item model adapting the Rust `Notebook` for QML.
//!
//! The `Notebook` owned by `NotebookModelRust` is the only copy of the data.
//! A `QModelIndex` stores the node's `NodeId` in its internal id (never a
//! pointer), and every use re-validates that id against the notebook, so a
//! stale index resolves to "no node" instead of dangling.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QModelIndex, QString, QUrl, QVariant};

use std::path::Path;

use crate::document::{with_default_extension, Document};
use crate::notebook::{NodeId, Notebook, NotebookError};
use crate::recovery::RecoveryError;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        include!("cxx-qt-lib/qtypes.h");
        type quintptr = cxx_qt_lib::quintptr;

        type QAbstractItemModel;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractItemModel]
        #[qproperty(bool, dirty, READ, NOTIFY)]
        #[qproperty(QString, document_name, cxx_name = "documentName", READ, NOTIFY)]
        #[qproperty(i32, recovery_revision, cxx_name = "recoveryRevision", READ, NOTIFY)]
        type NotebookModel = super::NotebookModelRust;
    }

    // Protected/base-class API needed to implement a hierarchical model.
    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "createIndex"]
        fn create_index(self: &NotebookModel, row: i32, column: i32, id: quintptr) -> QModelIndex;

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        fn begin_insert_rows(
            self: Pin<&mut NotebookModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endInsertRows"]
        fn end_insert_rows(self: Pin<&mut NotebookModel>);

        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        fn begin_remove_rows(
            self: Pin<&mut NotebookModel>,
            parent: &QModelIndex,
            first: i32,
            last: i32,
        );
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        fn end_remove_rows(self: Pin<&mut NotebookModel>);

        #[inherit]
        #[cxx_name = "beginMoveRows"]
        fn begin_move_rows(
            self: Pin<&mut NotebookModel>,
            source_parent: &QModelIndex,
            source_first: i32,
            source_last: i32,
            destination_parent: &QModelIndex,
            destination_child: i32,
        ) -> bool;
        #[inherit]
        #[cxx_name = "endMoveRows"]
        fn end_move_rows(self: Pin<&mut NotebookModel>);

        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut NotebookModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut NotebookModel>);
    }

    unsafe extern "RustQt" {
        /// A successful operation really changed the document: a note was
        /// created, a title or body actually changed, or a delete / Trash
        /// restore / checkpoint restore happened. Never emitted for
        /// unchanged text, failures, opening files, selection or viewing
        /// recovery history. Autosave restarts its timer on this.
        #[qsignal]
        #[cxx_name = "documentMutated"]
        fn document_mutated(self: Pin<&mut NotebookModel>);

        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut NotebookModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    // Standard QAbstractItemModel overrides.
    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn index(self: &NotebookModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        #[qinvokable]
        #[cxx_override]
        fn parent(self: &NotebookModel, child: &QModelIndex) -> QModelIndex;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &NotebookModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "columnCount"]
        fn column_count(self: &NotebookModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &NotebookModel, index: &QModelIndex, role: i32) -> QVariant;
    }

    // Operations callable from QML. Nodes are addressed by QModelIndex.
    unsafe extern "RustQt" {
        /// Appends a top-level note named "New note", "New note 2", ...
        /// (the first name no sibling has). Returns its index.
        #[qinvokable]
        #[cxx_name = "createDefaultRoot"]
        fn create_default_root(self: Pin<&mut NotebookModel>) -> QModelIndex;

        /// Appends a child under `parent`, named like `createDefaultRoot`.
        /// Returns its index, or an invalid index if `parent` is invalid or
        /// stale.
        #[qinvokable]
        #[cxx_name = "createDefaultChild"]
        fn create_default_child(self: Pin<&mut NotebookModel>, parent: &QModelIndex)
            -> QModelIndex;

        /// Renames a note. Returns an empty string on success, otherwise a
        /// message for the user (an empty name, a sibling that already has
        /// that name, or a stale index); on failure the title is unchanged.
        #[qinvokable]
        fn rename(self: Pin<&mut NotebookModel>, index: &QModelIndex, title: &QString) -> QString;

        /// How many groups of same-named siblings the notebook contains.
        /// Always zero for notebooks that only ever followed the naming rule;
        /// older files may have some.
        #[qinvokable]
        #[cxx_name = "titleConflictCount"]
        fn title_conflict_count(self: &NotebookModel) -> i32;

        /// Moves a node, with its subtree, under `parent` (an invalid index
        /// means top level) at final sibling position `position`. A position
        /// past the end means the last slot. Returns a fresh index for the
        /// node, whether it moved or was already there; an invalid index if
        /// the move is refused (missing or stale index, or a cycle). Any
        /// index held from before the move must not be reused.
        #[qinvokable]
        #[cxx_name = "moveNode"]
        fn move_node(
            self: Pin<&mut NotebookModel>,
            index: &QModelIndex,
            parent: &QModelIndex,
            position: i32,
        ) -> QModelIndex;

        /// Like `moveNode`, for drag and drop: `row` is the row of `parent`'s
        /// children the node is dropped *before*, as the user sees the tree
        /// now (so `rowCount(parent)` means "at the end").
        #[qinvokable]
        #[cxx_name = "dropNode"]
        fn drop_node(
            self: Pin<&mut NotebookModel>,
            index: &QModelIndex,
            parent: &QModelIndex,
            row: i32,
        ) -> QModelIndex;

        /// Whether the node may be placed under `parent` (invalid = top
        /// level): false for stale indexes and for itself or a descendant.
        #[qinvokable]
        #[cxx_name = "canDrop"]
        fn can_drop(self: &NotebookModel, index: &QModelIndex, parent: &QModelIndex) -> bool;

        /// Deletes the node and its subtree.
        #[qinvokable]
        #[cxx_name = "removeNode"]
        fn remove_node(self: Pin<&mut NotebookModel>, index: &QModelIndex) -> bool;

        /// The node's note text; empty if `index` is invalid or stale.
        #[qinvokable]
        fn body(self: &NotebookModel, index: &QModelIndex) -> QString;

        #[qinvokable]
        #[cxx_name = "setBody"]
        fn set_body(self: Pin<&mut NotebookModel>, index: &QModelIndex, body: &QString) -> bool;

        /// Replaces the notebook with the one stored at `path` (created
        /// empty if the path does not exist). Returns an empty string on
        /// success, otherwise a message for the user; on failure nothing
        /// changes.
        #[qinvokable]
        #[cxx_name = "openPath"]
        fn open_path(self: Pin<&mut NotebookModel>, path: &QString) -> QString;

        /// Opens an existing notebook chosen in a file dialog. Never creates
        /// a file. Returns an empty string on success, otherwise a message;
        /// on failure the current notebook is left completely unchanged.
        #[qinvokable]
        #[cxx_name = "openFile"]
        fn open_file(self: Pin<&mut NotebookModel>, url: &QUrl) -> QString;

        /// The local path a Save As dialog result refers to, with `.omatree`
        /// appended if no extension was given. Empty if `url` is not a local
        /// file.
        #[qinvokable]
        #[cxx_name = "saveAsTarget"]
        fn save_as_target(self: &NotebookModel, url: &QUrl) -> QString;

        /// Whether Save As to `path` would replace some other existing file.
        #[qinvokable]
        #[cxx_name = "needsOverwriteConfirmation"]
        fn needs_overwrite_confirmation(self: &NotebookModel, path: &QString) -> bool;

        /// Saves under `path` and switches to it only if that succeeds.
        /// `overwrite` must be true to replace an existing notebook. Returns
        /// an empty string on success, otherwise a message; on failure the
        /// path, name and dirty state are unchanged.
        #[qinvokable]
        #[cxx_name = "saveAs"]
        fn save_as(self: Pin<&mut NotebookModel>, path: &QString, overwrite: bool) -> QString;

        // Recovery. Entries are addressed by position in the lists below,
        // newest first, never by node id. Bind to `recoveryRevision` to
        // refresh a view when anything here changes.

        #[qinvokable]
        #[cxx_name = "trashCount"]
        fn trash_count(self: &NotebookModel) -> i32;

        #[qinvokable]
        #[cxx_name = "trashTitle"]
        fn trash_title(self: &NotebookModel, index: i32) -> QString;

        /// Seconds since the Unix epoch.
        #[qinvokable]
        #[cxx_name = "trashDeletedAt"]
        fn trash_deleted_at(self: &NotebookModel, index: i32) -> i64;

        /// How many notes the trashed subtree contains.
        #[qinvokable]
        #[cxx_name = "trashSize"]
        fn trash_size(self: &NotebookModel, index: i32) -> i32;

        /// Puts a trashed subtree back. Returns an empty string on success,
        /// otherwise a message; on failure nothing changes.
        #[qinvokable]
        #[cxx_name = "restoreTrash"]
        fn restore_trash(self: Pin<&mut NotebookModel>, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "checkpointCount"]
        fn checkpoint_count(self: &NotebookModel) -> i32;

        #[qinvokable]
        #[cxx_name = "checkpointReason"]
        fn checkpoint_reason(self: &NotebookModel, index: i32) -> QString;

        /// Seconds since the Unix epoch.
        #[qinvokable]
        #[cxx_name = "checkpointCreatedAt"]
        fn checkpoint_created_at(self: &NotebookModel, index: i32) -> i64;

        /// Replaces the whole notebook with a checkpoint (resetting the
        /// model). Leaves the document dirty; nothing is saved.
        #[qinvokable]
        #[cxx_name = "restoreCheckpoint"]
        fn restore_checkpoint(self: Pin<&mut NotebookModel>, index: i32) -> QString;

        /// Saves to the notebook's file. Returns an empty string on success,
        /// otherwise a message for the user; dirty stays set on failure.
        #[qinvokable]
        fn save(self: Pin<&mut NotebookModel>) -> QString;

        /// Whether the notebook has a file to save to.
        #[qinvokable]
        #[cxx_name = "hasPath"]
        fn has_path(self: &NotebookModel) -> bool;
    }
}

const DISPLAY_ROLE: i32 = 0;
const EDIT_ROLE: i32 = 2;

/// The model owns the single open `Document` (and through it the one
/// `Notebook`). `dirty` and `document_name` mirror it for QML.
pub struct NotebookModelRust {
    document: Document,
    dirty: bool,
    document_name: QString,
    recovery_revision: i32,
}

impl Default for NotebookModelRust {
    fn default() -> Self {
        let document = Document::untitled();
        let document_name = QString::from(document.display_name().as_str());
        NotebookModelRust {
            document,
            dirty: false,
            document_name,
            recovery_revision: 0,
        }
    }
}

/// Resolves a (row, column, internal id) triple to a live node.
///
/// Returns `None` if the id is unknown, the column is not 0, or the row no
/// longer matches the node's position (a stale index).
fn resolve(nb: &Notebook, row: i32, column: i32, internal_id: usize) -> Option<NodeId> {
    if column != 0 {
        return None;
    }
    let id = NodeId::from_raw(u64::try_from(internal_id).ok()?);
    let node = nb.get(id)?;
    (usize::try_from(row).ok()? == node.position()).then_some(id)
}

/// Child at `row` under `parent` (`None` = top level).
fn child_at(nb: &Notebook, parent: Option<NodeId>, row: i32) -> Option<NodeId> {
    let row = usize::try_from(row).ok()?;
    let siblings = match parent {
        None => nb.roots(),
        Some(p) => nb.children(p).ok()?,
    };
    siblings.get(row).map(|n| n.id())
}

fn child_count(nb: &Notebook, parent: Option<NodeId>) -> usize {
    match parent {
        None => nb.roots().len(),
        Some(p) => nb.children(p).map_or(0, |c| c.len()),
    }
}

/// Maps a position in a newest-first list to an index into the stored
/// oldest-first list.
fn nth_newest(len: usize, position: i32) -> Option<usize> {
    let position = usize::try_from(position).ok()?;
    (position < len).then(|| len - 1 - position)
}

/// What to tell the user when a rename was refused.
fn rename_error_message(error: NotebookError, title: &str) -> String {
    match error {
        NotebookError::TitleConflict => {
            format!("A note named \"{}\" already exists here.", title.trim())
        }
        NotebookError::EmptyTitle => "A note needs a name.".to_string(),
        _ => "That note could not be renamed.".to_string(),
    }
}

fn count_to_i32(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(i32::MAX)
}

fn position(nb: &Notebook, id: NodeId) -> Option<i32> {
    i32::try_from(nb.get(id)?.position()).ok()
}

impl qobject::NotebookModel {
    fn node_for(&self, index: &QModelIndex) -> Option<NodeId> {
        if !index.is_valid() {
            return None;
        }
        resolve(
            self.rust().document.notebook(),
            index.row(),
            index.column(),
            index.internal_id(),
        )
    }

    fn index_for(&self, id: NodeId) -> QModelIndex {
        let nb = self.rust().document.notebook();
        match (position(nb, id), usize::try_from(id.get())) {
            (Some(row), Ok(raw)) => self.create_index(row, 0, raw.into()),
            _ => QModelIndex::default(),
        }
    }

    /// Parent as `Some(None)` for top level, `Some(Some(id))` for a live
    /// node, `None` for an invalid or stale index.
    fn parent_for(&self, index: &QModelIndex) -> Option<Option<NodeId>> {
        if !index.is_valid() {
            return Some(None);
        }
        self.node_for(index).map(Some)
    }

    fn index(&self, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex {
        if column != 0 {
            return QModelIndex::default();
        }
        let Some(parent) = self.parent_for(parent) else {
            return QModelIndex::default();
        };
        match child_at(self.rust().document.notebook(), parent, row) {
            Some(id) => self.index_for(id),
            None => QModelIndex::default(),
        }
    }

    fn parent(&self, child: &QModelIndex) -> QModelIndex {
        let Some(id) = self.node_for(child) else {
            return QModelIndex::default();
        };
        match self
            .rust()
            .document
            .notebook()
            .get(id)
            .and_then(|n| n.parent_id())
        {
            Some(parent) => self.index_for(parent),
            None => QModelIndex::default(),
        }
    }

    fn row_count(&self, parent: &QModelIndex) -> i32 {
        let Some(parent) = self.parent_for(parent) else {
            return 0;
        };
        i32::try_from(child_count(self.rust().document.notebook(), parent)).unwrap_or(i32::MAX)
    }

    fn column_count(&self, _parent: &QModelIndex) -> i32 {
        1
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if role != DISPLAY_ROLE && role != EDIT_ROLE {
            return QVariant::default();
        }
        match self
            .node_for(index)
            .and_then(|id| self.rust().document.notebook().get(id))
        {
            Some(node) => QVariant::from(&QString::from(node.title())),
            None => QVariant::default(),
        }
    }

    /// Pushes the document's dirty flag and name to QML.
    fn sync_state(mut self: Pin<&mut Self>) {
        let dirty = self.rust().document.is_dirty();
        if self.rust().dirty != dirty {
            self.as_mut().rust_mut().dirty = dirty;
            self.as_mut().dirty_changed();
        }
        let name = QString::from(self.rust().document.display_name().as_str());
        if self.rust().document_name != name {
            self.as_mut().rust_mut().document_name = name;
            self.as_mut().document_name_changed();
        }
    }

    /// Tells QML the Trash or checkpoint lists may have changed.
    fn bump_recovery(mut self: Pin<&mut Self>) {
        let next = self.rust().recovery_revision.wrapping_add(1);
        self.as_mut().rust_mut().recovery_revision = next;
        self.as_mut().recovery_revision_changed();
    }

    fn trash_count(&self) -> i32 {
        count_to_i32(self.rust().document.recovery().trash().len())
    }

    fn trash_entry(&self, index: i32) -> Option<&crate::recovery::TrashEntry> {
        let trash = self.rust().document.recovery().trash();
        nth_newest(trash.len(), index).map(|i| &trash[i])
    }

    fn trash_title(&self, index: i32) -> QString {
        self.trash_entry(index)
            .map_or_else(QString::default, |e| QString::from(e.title()))
    }

    fn trash_deleted_at(&self, index: i32) -> i64 {
        self.trash_entry(index).map_or(0, |e| e.deleted_at())
    }

    fn trash_size(&self, index: i32) -> i32 {
        self.trash_entry(index)
            .map_or(0, |e| count_to_i32(e.node_count()))
    }

    fn restore_trash(mut self: Pin<&mut Self>, index: i32) -> QString {
        let len = self.rust().document.recovery().trash().len();
        let Some(stored) = nth_newest(len, index) else {
            return QString::from(RecoveryError::NoSuchEntry.message());
        };
        // The restored root appears as one inserted row (its descendants come
        // with it), so announce exactly that row.
        let (parent, row) = match self.rust().document.plan_trash_restore(stored) {
            Ok(placement) => placement,
            Err(e) => return QString::from(e.message()),
        };
        let parent_index = parent.map_or_else(QModelIndex::default, |id| self.index_for(id));
        let row = count_to_i32(row);
        self.as_mut().begin_insert_rows(&parent_index, row, row);
        let result = self.as_mut().rust_mut().document.restore_trash(stored);
        self.as_mut().end_insert_rows();
        self.as_mut().sync_state();
        self.as_mut().bump_recovery();
        match result {
            Ok(_) => {
                self.as_mut().document_mutated();
                QString::default()
            }
            Err(e) => QString::from(e.message()),
        }
    }

    fn checkpoint_count(&self) -> i32 {
        count_to_i32(self.rust().document.recovery().checkpoints().len())
    }

    fn checkpoint_entry(&self, index: i32) -> Option<&crate::recovery::Checkpoint> {
        let list = self.rust().document.recovery().checkpoints();
        nth_newest(list.len(), index).map(|i| &list[i])
    }

    fn checkpoint_reason(&self, index: i32) -> QString {
        self.checkpoint_entry(index)
            .map_or_else(QString::default, |c| QString::from(c.reason()))
    }

    fn checkpoint_created_at(&self, index: i32) -> i64 {
        self.checkpoint_entry(index).map_or(0, |c| c.created_at())
    }

    fn restore_checkpoint(mut self: Pin<&mut Self>, index: i32) -> QString {
        let len = self.rust().document.recovery().checkpoints().len();
        let Some(stored) = nth_newest(len, index) else {
            return QString::from(RecoveryError::NoSuchEntry.message());
        };
        // The whole tree is replaced, so this is a genuine model reset. QML
        // clears the selection on `modelAboutToBeReset`.
        self.as_mut().begin_reset_model();
        let result = self.as_mut().rust_mut().document.restore_checkpoint(stored);
        self.as_mut().end_reset_model();
        self.as_mut().sync_state();
        self.as_mut().bump_recovery();
        match result {
            Ok(()) => {
                self.as_mut().document_mutated();
                QString::default()
            }
            Err(e) => QString::from(e.message()),
        }
    }

    /// The target parent of a move: `Some(None)` for top level (invalid
    /// index), `Some(Some(id))` for a live node, `None` for a stale index.
    fn move_target(&self, parent: &QModelIndex) -> Option<Option<NodeId>> {
        self.parent_for(parent)
    }

    fn can_drop(&self, index: &QModelIndex, parent: &QModelIndex) -> bool {
        match (self.node_for(index), self.move_target(parent)) {
            (Some(id), Some(parent)) => self.rust().document.notebook().can_reparent(id, parent),
            _ => false,
        }
    }

    fn move_node(
        mut self: Pin<&mut Self>,
        index: &QModelIndex,
        parent: &QModelIndex,
        position: i32,
    ) -> QModelIndex {
        let (Some(id), Some(parent)) = (self.node_for(index), self.move_target(parent)) else {
            return QModelIndex::default();
        };
        let Ok(position) = usize::try_from(position) else {
            return QModelIndex::default();
        };
        self.as_mut().apply_move(id, parent, position)
    }

    fn drop_node(
        mut self: Pin<&mut Self>,
        index: &QModelIndex,
        parent: &QModelIndex,
        row: i32,
    ) -> QModelIndex {
        let (Some(id), Some(parent)) = (self.node_for(index), self.move_target(parent)) else {
            return QModelIndex::default();
        };
        let Ok(slot) = usize::try_from(row) else {
            return QModelIndex::default();
        };
        // Dropping "before row N" counts rows with the dragged node still in
        // place. Removing it from above N shifts the slot up by one; the
        // pure notebook only ever sees final positions.
        let final_position = match self.rust().document.notebook().get(id) {
            Some(node) if node.parent_id() == parent && node.position() < slot => slot - 1,
            _ => slot,
        };
        self.as_mut().apply_move(id, parent, final_position)
    }

    /// Performs a validated move with proper row-move notifications and
    /// returns a fresh index for the node (invalid if refused).
    fn apply_move(
        mut self: Pin<&mut Self>,
        id: NodeId,
        parent: Option<NodeId>,
        position: usize,
    ) -> QModelIndex {
        let plan = match self.rust().document.plan_move(id, parent, position) {
            Ok(Some(plan)) => plan,
            Ok(None) => return self.index_for(id), // already there: nothing happens
            Err(_) => return QModelIndex::default(),
        };
        let index_of = |this: &Self, node: Option<NodeId>| {
            node.map_or_else(QModelIndex::default, |n| this.index_for(n))
        };
        let source_parent = index_of(&self, plan.old_parent);
        let destination_parent = index_of(&self, plan.new_parent);
        let row = count_to_i32(plan.old_position);
        // Qt's destination row is where the rows are inserted, counted before
        // they are taken out. Moving down inside one parent therefore needs
        // one more than the final position; moving up or to another parent
        // uses the final position as it is.
        let moving_down =
            plan.old_parent == plan.new_parent && plan.new_position > plan.old_position;
        let destination = count_to_i32(plan.new_position + usize::from(moving_down));

        if !self.as_mut().begin_move_rows(
            &source_parent,
            row,
            row,
            &destination_parent,
            destination,
        ) {
            return QModelIndex::default();
        }
        let result = self
            .as_mut()
            .rust_mut()
            .document
            .move_node(id, parent, position);
        self.as_mut().end_move_rows();
        self.as_mut().sync_state();
        self.as_mut().bump_recovery();
        if matches!(result, Ok(true)) {
            self.as_mut().document_mutated();
        }
        self.index_for(id)
    }

    fn create_default_root(mut self: Pin<&mut Self>) -> QModelIndex {
        let row = child_count(self.rust().document.notebook(), None);
        let row = i32::try_from(row).unwrap_or(i32::MAX);
        self.as_mut()
            .begin_insert_rows(&QModelIndex::default(), row, row);
        let created = self.as_mut().rust_mut().document.create_default_root();
        self.as_mut().end_insert_rows();
        self.as_mut().sync_state();
        match created {
            Ok(id) => {
                self.as_mut().document_mutated();
                self.index_for(id)
            }
            Err(_) => QModelIndex::default(),
        }
    }

    fn create_default_child(mut self: Pin<&mut Self>, parent: &QModelIndex) -> QModelIndex {
        // An invalid parent is an error here, not "top level".
        let Some(parent_id) = self.node_for(parent) else {
            return QModelIndex::default();
        };
        let row = child_count(self.rust().document.notebook(), Some(parent_id));
        let row = i32::try_from(row).unwrap_or(i32::MAX);
        self.as_mut().begin_insert_rows(parent, row, row);
        let created = self
            .as_mut()
            .rust_mut()
            .document
            .create_default_child(parent_id);
        self.as_mut().end_insert_rows();
        self.as_mut().sync_state();
        match created {
            Ok(id) => {
                self.as_mut().document_mutated();
                self.index_for(id)
            }
            Err(_) => QModelIndex::default(),
        }
    }

    fn rename(mut self: Pin<&mut Self>, index: &QModelIndex, title: &QString) -> QString {
        let Some(id) = self.node_for(index) else {
            return QString::from("That note is no longer available.");
        };
        let title = String::from(title);
        let changed = match self.as_mut().rust_mut().document.rename(id, &title) {
            Ok(changed) => changed,
            Err(e) => {
                return QString::from(rename_error_message(e, &title).as_str());
            }
        };
        let index = self.index_for(id);
        self.as_mut()
            .data_changed(&index, &index, &QList::<i32>::default());
        self.as_mut().sync_state();
        if changed {
            self.as_mut().document_mutated();
        }
        QString::default()
    }

    fn title_conflict_count(&self) -> i32 {
        count_to_i32(self.rust().document.notebook().sibling_title_conflicts())
    }

    fn remove_node(mut self: Pin<&mut Self>, index: &QModelIndex) -> bool {
        let Some(id) = self.node_for(index) else {
            return false;
        };
        let Some(row) = position(self.rust().document.notebook(), id) else {
            return false;
        };
        let parent = self.parent(&self.index_for(id));
        self.as_mut().begin_remove_rows(&parent, row, row);
        let removed = self.as_mut().rust_mut().document.delete(id).is_ok();
        self.as_mut().end_remove_rows();
        self.as_mut().sync_state();
        self.as_mut().bump_recovery();
        if removed {
            self.as_mut().document_mutated();
        }
        removed
    }

    fn body(&self, index: &QModelIndex) -> QString {
        match self
            .node_for(index)
            .and_then(|id| self.rust().document.notebook().get(id))
        {
            Some(node) => QString::from(node.body()),
            None => QString::default(),
        }
    }

    fn set_body(mut self: Pin<&mut Self>, index: &QModelIndex, body: &QString) -> bool {
        let Some(id) = self.node_for(index) else {
            return false;
        };
        let Ok(changed) = self
            .as_mut()
            .rust_mut()
            .document
            .set_body(id, &String::from(body))
        else {
            return false;
        };
        self.as_mut().sync_state();
        if changed {
            self.as_mut().document_mutated();
        }
        true
    }

    /// Swaps in a fully built document. Replacing the whole notebook is the
    /// one legitimate model reset.
    fn replace_document(mut self: Pin<&mut Self>, document: Document) {
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().document = document;
        self.as_mut().end_reset_model();
        self.as_mut().sync_state();
        self.as_mut().bump_recovery();
    }

    fn open_path(self: Pin<&mut Self>, path: &QString) -> QString {
        let path = String::from(path);
        self.finish_open(&path, Document::open(Path::new(&path)))
    }

    fn open_file(self: Pin<&mut Self>, url: &QUrl) -> QString {
        let Some(path) = url.to_local_file().map(String::from) else {
            return QString::from("That location is not a local file.");
        };
        self.finish_open(&path, Document::open_existing(Path::new(&path)))
    }

    fn finish_open(
        self: Pin<&mut Self>,
        path: &str,
        result: Result<Document, crate::document::DocumentError>,
    ) -> QString {
        match result {
            Err(e) => {
                eprintln!("omatree: could not open {path}: {e}");
                QString::from(e.open_message().as_str())
            }
            Ok(document) => {
                self.replace_document(document);
                QString::default()
            }
        }
    }

    fn save_as_target(&self, url: &QUrl) -> QString {
        match url.to_local_file() {
            Some(path) => {
                let path = with_default_extension(Path::new(&String::from(path)));
                QString::from(path.to_string_lossy().as_ref())
            }
            None => QString::default(),
        }
    }

    fn needs_overwrite_confirmation(&self, path: &QString) -> bool {
        self.rust()
            .document
            .save_as_needs_confirmation(Path::new(&String::from(path)))
    }

    fn save_as(mut self: Pin<&mut Self>, path: &QString, overwrite: bool) -> QString {
        let path = String::from(path);
        let result = self
            .as_mut()
            .rust_mut()
            .document
            .save_as(Path::new(&path), overwrite);
        self.as_mut().sync_state();
        match result {
            Ok(()) => QString::default(),
            Err(e) => {
                eprintln!("omatree: save as {path} failed: {e}");
                QString::from(e.save_as_message().as_str())
            }
        }
    }

    fn save(mut self: Pin<&mut Self>) -> QString {
        let result = self.as_mut().rust_mut().document.save();
        self.as_mut().sync_state();
        match result {
            Ok(()) => QString::default(),
            Err(e) => {
                eprintln!("omatree: save failed: {e}");
                QString::from(e.save_message().as_str())
            }
        }
    }

    fn has_path(&self) -> bool {
        self.rust().document.has_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Projects
    /// ├── OmaTree
    /// │   └── Ideas
    /// └── Threatwright
    /// Inbox
    fn sample() -> (Notebook, [NodeId; 5]) {
        let mut nb = Notebook::new();
        let projects = nb.create_root("Projects").unwrap();
        let inbox = nb.create_root("Inbox").unwrap();
        let omatree = nb.create_child(projects, "OmaTree").unwrap();
        let threat = nb.create_child(projects, "Threatwright").unwrap();
        let ideas = nb.create_child(omatree, "Ideas").unwrap();
        (nb, [projects, inbox, omatree, threat, ideas])
    }

    /// What `index_for` stores in a QModelIndex for `id`.
    fn index_parts(nb: &Notebook, id: NodeId) -> (i32, i32, usize) {
        (position(nb, id).unwrap(), 0, id.get() as usize)
    }

    #[test]
    fn recovery_lists_are_addressed_newest_first() {
        assert_eq!(nth_newest(3, 0), Some(2));
        assert_eq!(nth_newest(3, 1), Some(1));
        assert_eq!(nth_newest(3, 2), Some(0));
        assert_eq!(nth_newest(3, 3), None);
        assert_eq!(nth_newest(3, -1), None);
        assert_eq!(nth_newest(0, 0), None);
    }

    #[test]
    fn root_row_count_and_order() {
        let (nb, [projects, inbox, ..]) = sample();
        assert_eq!(child_count(&nb, None), 2);
        assert_eq!(child_at(&nb, None, 0), Some(projects));
        assert_eq!(child_at(&nb, None, 1), Some(inbox));
        assert_eq!(child_at(&nb, None, 2), None);
    }

    #[test]
    fn child_row_count_and_sibling_order() {
        let (nb, [projects, inbox, omatree, threat, ideas]) = sample();
        assert_eq!(child_count(&nb, Some(projects)), 2);
        assert_eq!(child_count(&nb, Some(inbox)), 0);
        assert_eq!(child_at(&nb, Some(projects), 0), Some(omatree));
        assert_eq!(child_at(&nb, Some(projects), 1), Some(threat));
        assert_eq!(child_at(&nb, Some(omatree), 0), Some(ideas));
    }

    #[test]
    fn indexes_round_trip_to_nodes() {
        let (nb, ids) = sample();
        for id in ids {
            let (row, col, raw) = index_parts(&nb, id);
            assert_eq!(resolve(&nb, row, col, raw), Some(id));
        }
    }

    #[test]
    fn parent_child_relationships_are_consistent() {
        let (nb, [projects, _, omatree, _, ideas]) = sample();
        let parent_of = |id| nb.get(id).unwrap().parent_id();
        assert_eq!(parent_of(ideas), Some(omatree));
        assert_eq!(parent_of(omatree), Some(projects));
        assert_eq!(parent_of(projects), None);
        // Row of each child under its parent matches its position.
        assert_eq!(
            child_at(&nb, Some(omatree), position(&nb, ideas).unwrap()),
            Some(ideas)
        );
    }

    #[test]
    fn invalid_and_stale_indexes_resolve_to_none() {
        let (mut nb, [projects, _, omatree, threat, _]) = sample();
        assert_eq!(resolve(&nb, 0, 1, projects.get() as usize), None, "column");
        assert_eq!(resolve(&nb, -1, 0, projects.get() as usize), None, "row");
        assert_eq!(resolve(&nb, 0, 0, 9999), None, "unknown id");
        assert_eq!(resolve(&nb, 0, 0, usize::MAX), None, "huge id");
        assert_eq!(child_at(&nb, None, -1), None);
        assert_eq!(child_at(&nb, Some(NodeId::from_raw(9999)), 0), None);
        assert_eq!(child_count(&nb, Some(NodeId::from_raw(9999))), 0);

        // Index taken before a delete: the node's row shifts, id is gone.
        let stale_omatree = index_parts(&nb, omatree);
        let stale_threat = index_parts(&nb, threat);
        nb.delete(omatree).unwrap();
        let (r, c, id) = stale_omatree;
        assert_eq!(resolve(&nb, r, c, id), None, "deleted node");
        let (r, c, id) = stale_threat;
        assert_eq!(resolve(&nb, r, c, id), None, "row shifted from 1 to 0");
        assert_eq!(resolve(&nb, 0, 0, threat.get() as usize), Some(threat));
    }

    #[test]
    fn delete_removes_subtree_from_model_view() {
        let (mut nb, [projects, inbox, omatree, _, ideas]) = sample();
        nb.delete(projects).unwrap();
        assert_eq!(child_count(&nb, None), 1);
        assert_eq!(child_at(&nb, None, 0), Some(inbox));
        for gone in [projects, omatree, ideas] {
            assert_eq!(resolve(&nb, 0, 0, gone.get() as usize), None);
        }
    }
}
