//! Qt item model adapting the Rust `Notebook` for QML.
//!
//! The `Notebook` owned by `NotebookModelRust` is the only copy of the data.
//! A `QModelIndex` stores the node's `NodeId` in its internal id (never a
//! pointer), and every use re-validates that id against the notebook, so a
//! stale index resolves to "no node" instead of dangling.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QModelIndex, QString, QVariant};

use crate::notebook::{NodeId, Notebook};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
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
    }

    unsafe extern "RustQt" {
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
        /// Appends a root node. Returns its index.
        #[qinvokable]
        #[cxx_name = "createRoot"]
        fn create_root(self: Pin<&mut NotebookModel>, title: &QString) -> QModelIndex;

        /// Appends a child under `parent`. Returns its index, or an invalid
        /// index if `parent` is invalid or stale.
        #[qinvokable]
        #[cxx_name = "createChild"]
        fn create_child(
            self: Pin<&mut NotebookModel>,
            parent: &QModelIndex,
            title: &QString,
        ) -> QModelIndex;

        #[qinvokable]
        fn rename(self: Pin<&mut NotebookModel>, index: &QModelIndex, title: &QString) -> bool;

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
    }
}

const DISPLAY_ROLE: i32 = 0;
const EDIT_ROLE: i32 = 2;

#[derive(Default)]
pub struct NotebookModelRust {
    notebook: Notebook,
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

fn position(nb: &Notebook, id: NodeId) -> Option<i32> {
    i32::try_from(nb.get(id)?.position()).ok()
}

impl qobject::NotebookModel {
    fn node_for(&self, index: &QModelIndex) -> Option<NodeId> {
        if !index.is_valid() {
            return None;
        }
        resolve(
            &self.rust().notebook,
            index.row(),
            index.column(),
            index.internal_id(),
        )
    }

    fn index_for(&self, id: NodeId) -> QModelIndex {
        let nb = &self.rust().notebook;
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
        match child_at(&self.rust().notebook, parent, row) {
            Some(id) => self.index_for(id),
            None => QModelIndex::default(),
        }
    }

    fn parent(&self, child: &QModelIndex) -> QModelIndex {
        let Some(id) = self.node_for(child) else {
            return QModelIndex::default();
        };
        match self.rust().notebook.get(id).and_then(|n| n.parent_id()) {
            Some(parent) => self.index_for(parent),
            None => QModelIndex::default(),
        }
    }

    fn row_count(&self, parent: &QModelIndex) -> i32 {
        let Some(parent) = self.parent_for(parent) else {
            return 0;
        };
        i32::try_from(child_count(&self.rust().notebook, parent)).unwrap_or(i32::MAX)
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
            .and_then(|id| self.rust().notebook.get(id))
        {
            Some(node) => QVariant::from(&QString::from(node.title())),
            None => QVariant::default(),
        }
    }

    fn create_root(mut self: Pin<&mut Self>, title: &QString) -> QModelIndex {
        let row = child_count(&self.rust().notebook, None);
        let row = i32::try_from(row).unwrap_or(i32::MAX);
        self.as_mut()
            .begin_insert_rows(&QModelIndex::default(), row, row);
        let id = self
            .as_mut()
            .rust_mut()
            .notebook
            .create_root(&String::from(title));
        self.as_mut().end_insert_rows();
        self.index_for(id)
    }

    fn create_child(
        mut self: Pin<&mut Self>,
        parent: &QModelIndex,
        title: &QString,
    ) -> QModelIndex {
        // An invalid parent is an error here, not "top level".
        let Some(parent_id) = self.node_for(parent) else {
            return QModelIndex::default();
        };
        let row = child_count(&self.rust().notebook, Some(parent_id));
        let row = i32::try_from(row).unwrap_or(i32::MAX);
        self.as_mut().begin_insert_rows(parent, row, row);
        let created = self
            .as_mut()
            .rust_mut()
            .notebook
            .create_child(parent_id, &String::from(title));
        self.as_mut().end_insert_rows();
        match created {
            Ok(id) => self.index_for(id),
            Err(_) => QModelIndex::default(),
        }
    }

    fn rename(mut self: Pin<&mut Self>, index: &QModelIndex, title: &QString) -> bool {
        let Some(id) = self.node_for(index) else {
            return false;
        };
        if self
            .as_mut()
            .rust_mut()
            .notebook
            .rename(id, &String::from(title))
            .is_err()
        {
            return false;
        }
        let index = self.index_for(id);
        self.as_mut()
            .data_changed(&index, &index, &QList::<i32>::default());
        true
    }

    fn remove_node(mut self: Pin<&mut Self>, index: &QModelIndex) -> bool {
        let Some(id) = self.node_for(index) else {
            return false;
        };
        let Some(row) = position(&self.rust().notebook, id) else {
            return false;
        };
        let parent = self.parent(&self.index_for(id));
        self.as_mut().begin_remove_rows(&parent, row, row);
        let removed = self.as_mut().rust_mut().notebook.delete(id).is_ok();
        self.as_mut().end_remove_rows();
        removed
    }

    fn body(&self, index: &QModelIndex) -> QString {
        match self
            .node_for(index)
            .and_then(|id| self.rust().notebook.get(id))
        {
            Some(node) => QString::from(node.body()),
            None => QString::default(),
        }
    }

    fn set_body(mut self: Pin<&mut Self>, index: &QModelIndex, body: &QString) -> bool {
        let Some(id) = self.node_for(index) else {
            return false;
        };
        self.as_mut()
            .rust_mut()
            .notebook
            .set_body(id, &String::from(body))
            .is_ok()
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
        let projects = nb.create_root("Projects");
        let inbox = nb.create_root("Inbox");
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
