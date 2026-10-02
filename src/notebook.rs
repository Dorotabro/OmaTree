//! In-memory notebook and node model. Pure Rust, no Qt.

use std::collections::BTreeMap;
use std::fmt;

/// Stable identifier of a node. Never reused within a notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A single note. Read-only to callers; only `Notebook` mutates nodes, so
/// parent/position invariants cannot be broken from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    id: NodeId,
    parent_id: Option<NodeId>,
    position: usize,
    title: String,
    body: String,
}

impl Node {
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// `None` for root nodes.
    pub fn parent_id(&self) -> Option<NodeId> {
        self.parent_id
    }

    /// Zero-based index among siblings.
    pub fn position(&self) -> usize {
        self.position
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body(&self) -> &str {
        &self.body
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookError {
    NodeNotFound(NodeId),
}

impl fmt::Display for NotebookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotebookError::NodeNotFound(id) => write!(f, "node {} does not exist", id.0),
        }
    }
}

impl std::error::Error for NotebookError {}

#[derive(Debug, Default)]
pub struct Notebook {
    nodes: BTreeMap<NodeId, Node>,
    next_id: u64,
}

impl Notebook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_root(&mut self, title: &str) -> NodeId {
        self.insert(None, title)
    }

    pub fn create_child(&mut self, parent: NodeId, title: &str) -> Result<NodeId, NotebookError> {
        self.require(parent)?;
        Ok(self.insert(Some(parent), title))
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn rename(&mut self, id: NodeId, title: &str) -> Result<(), NotebookError> {
        self.require_mut(id)?.title = title.to_string();
        Ok(())
    }

    pub fn set_body(&mut self, id: NodeId, body: &str) -> Result<(), NotebookError> {
        self.require_mut(id)?.body = body.to_string();
        Ok(())
    }

    /// Root nodes in sibling order.
    pub fn roots(&self) -> Vec<&Node> {
        self.siblings(None)
    }

    /// Children of `id` in sibling order.
    pub fn children(&self, id: NodeId) -> Result<Vec<&Node>, NotebookError> {
        self.require(id)?;
        Ok(self.siblings(Some(id)))
    }

    /// Deletes a node and all of its descendants. Remaining siblings are
    /// renumbered so positions stay contiguous.
    pub fn delete(&mut self, id: NodeId) -> Result<(), NotebookError> {
        let parent = self.require(id)?.parent_id;

        let mut doomed = vec![id];
        let mut i = 0;
        while i < doomed.len() {
            let current = doomed[i];
            doomed.extend(self.siblings(Some(current)).iter().map(|n| n.id));
            i += 1;
        }
        for id in doomed {
            self.nodes.remove(&id);
        }

        let remaining: Vec<NodeId> = self.siblings(parent).iter().map(|n| n.id).collect();
        for (position, id) in remaining.into_iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.position = position;
            }
        }
        Ok(())
    }

    fn insert(&mut self, parent_id: Option<NodeId>, title: &str) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        let position = self.siblings(parent_id).len();
        self.nodes.insert(
            id,
            Node {
                id,
                parent_id,
                position,
                title: title.to_string(),
                body: String::new(),
            },
        );
        id
    }

    fn siblings(&self, parent_id: Option<NodeId>) -> Vec<&Node> {
        let mut nodes: Vec<&Node> = self
            .nodes
            .values()
            .filter(|n| n.parent_id == parent_id)
            .collect();
        nodes.sort_by_key(|n| n.position);
        nodes
    }

    fn require(&self, id: NodeId) -> Result<&Node, NotebookError> {
        self.nodes.get(&id).ok_or(NotebookError::NodeNotFound(id))
    }

    fn require_mut(&mut self, id: NodeId) -> Result<&mut Node, NotebookError> {
        self.nodes
            .get_mut(&id)
            .ok_or(NotebookError::NodeNotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(nodes: Vec<&Node>) -> Vec<&str> {
        nodes.iter().map(|n| n.title()).collect()
    }

    #[test]
    fn creates_multiple_roots() {
        let mut nb = Notebook::new();
        let a = nb.create_root("A");
        let b = nb.create_root("B");
        assert_ne!(a, b);
        assert_eq!(nb.get(a).unwrap().parent_id(), None);
        assert_eq!(nb.get(b).unwrap().title(), "B");
        assert_eq!(nb.roots().len(), 2);
    }

    #[test]
    fn creates_nested_children() {
        let mut nb = Notebook::new();
        let root = nb.create_root("Projects");
        let child = nb.create_child(root, "OmaTree").unwrap();
        let grandchild = nb.create_child(child, "Ideas").unwrap();
        assert_eq!(nb.get(child).unwrap().parent_id(), Some(root));
        assert_eq!(nb.get(grandchild).unwrap().parent_id(), Some(child));
        assert_eq!(titles(nb.children(root).unwrap()), ["OmaTree"]);
        assert_eq!(titles(nb.children(child).unwrap()), ["Ideas"]);
        assert!(nb.children(grandchild).unwrap().is_empty());
    }

    #[test]
    fn roots_are_in_creation_order() {
        let mut nb = Notebook::new();
        nb.create_root("one");
        nb.create_root("two");
        nb.create_root("three");
        assert_eq!(titles(nb.roots()), ["one", "two", "three"]);
        let positions: Vec<usize> = nb.roots().iter().map(|n| n.position()).collect();
        assert_eq!(positions, [0, 1, 2]);
    }

    #[test]
    fn children_are_in_creation_order() {
        let mut nb = Notebook::new();
        let root = nb.create_root("root");
        nb.create_child(root, "x").unwrap();
        nb.create_child(root, "y").unwrap();
        nb.create_child(root, "z").unwrap();
        assert_eq!(titles(nb.children(root).unwrap()), ["x", "y", "z"]);
    }

    #[test]
    fn renames_a_node() {
        let mut nb = Notebook::new();
        let id = nb.create_root("old");
        nb.rename(id, "new").unwrap();
        assert_eq!(nb.get(id).unwrap().title(), "new");
    }

    #[test]
    fn updates_body_text() {
        let mut nb = Notebook::new();
        let id = nb.create_root("note");
        assert_eq!(nb.get(id).unwrap().body(), "");
        nb.set_body(id, "hello\nworld").unwrap();
        assert_eq!(nb.get(id).unwrap().body(), "hello\nworld");
    }

    #[test]
    fn deletes_a_leaf_and_renumbers_siblings() {
        let mut nb = Notebook::new();
        let root = nb.create_root("root");
        nb.create_child(root, "a").unwrap();
        let b = nb.create_child(root, "b").unwrap();
        nb.create_child(root, "c").unwrap();
        nb.delete(b).unwrap();
        assert!(nb.get(b).is_none());
        let children = nb.children(root).unwrap();
        assert_eq!(titles(children.clone()), ["a", "c"]);
        let positions: Vec<usize> = children.iter().map(|n| n.position()).collect();
        assert_eq!(positions, [0, 1]);
    }

    #[test]
    fn delete_removes_all_descendants() {
        let mut nb = Notebook::new();
        let keep = nb.create_root("keep");
        let root = nb.create_root("root");
        let child = nb.create_child(root, "child").unwrap();
        let grandchild = nb.create_child(child, "grandchild").unwrap();
        nb.delete(root).unwrap();
        assert!(nb.get(root).is_none());
        assert!(nb.get(child).is_none());
        assert!(nb.get(grandchild).is_none());
        assert_eq!(titles(nb.roots()), ["keep"]);
        assert!(nb.get(keep).is_some());
    }

    #[test]
    fn ids_are_not_reused_after_delete() {
        let mut nb = Notebook::new();
        let a = nb.create_root("a");
        nb.delete(a).unwrap();
        let b = nb.create_root("b");
        assert_ne!(a, b);
    }

    #[test]
    fn invalid_ids_fail_cleanly() {
        let mut nb = Notebook::new();
        let id = nb.create_root("gone");
        nb.delete(id).unwrap();
        let missing = NotebookError::NodeNotFound(id);
        assert_eq!(nb.create_child(id, "orphan"), Err(missing));
        assert_eq!(nb.rename(id, "x"), Err(missing));
        assert_eq!(nb.set_body(id, "x"), Err(missing));
        assert_eq!(nb.children(id).err(), Some(missing));
        assert_eq!(nb.delete(id), Err(missing));
        assert!(nb.roots().is_empty());
    }
}
