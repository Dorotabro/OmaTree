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

    /// For rebuilding persisted ids; fresh ids only come from `Notebook`.
    pub(crate) fn from_raw(raw: u64) -> Self {
        NodeId(raw)
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
    /// Unvalidated; only meant to be passed to `Notebook::from_nodes`.
    pub(crate) fn from_parts(
        id: NodeId,
        parent_id: Option<NodeId>,
        position: usize,
        title: String,
        body: String,
    ) -> Self {
        Node {
            id,
            parent_id,
            position,
            title,
            body,
        }
    }

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
    /// Nodes handed to `Notebook::from_nodes` do not form a valid tree.
    InvalidStructure(&'static str),
    /// A node being restored has the same id as an active node.
    IdCollision(NodeId),
}

impl fmt::Display for NotebookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotebookError::NodeNotFound(id) => write!(f, "node {} does not exist", id.0),
            NotebookError::InvalidStructure(why) => write!(f, "invalid notebook: {why}"),
            NotebookError::IdCollision(id) => write!(f, "node {} already exists", id.0),
        }
    }
}

impl std::error::Error for NotebookError {}

#[derive(Debug, Default, Clone)]
pub struct Notebook {
    nodes: BTreeMap<NodeId, Node>,
    next_id: u64,
}

impl Notebook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a notebook from existing nodes, preserving their ids.
    ///
    /// Fails unless ids are unique, every parent exists, the nodes form a
    /// tree (no cycles), and each sibling group has positions `0..n`.
    /// New ids continue after the highest existing one.
    pub(crate) fn from_nodes(nodes: Vec<Node>) -> Result<Self, NotebookError> {
        let mut map = BTreeMap::new();
        for node in nodes {
            if map.insert(node.id, node).is_some() {
                return Err(NotebookError::InvalidStructure("duplicate node id"));
            }
        }

        let mut groups: BTreeMap<Option<NodeId>, Vec<usize>> = BTreeMap::new();
        for node in map.values() {
            if let Some(parent) = node.parent_id {
                if !map.contains_key(&parent) {
                    return Err(NotebookError::InvalidStructure("parent does not exist"));
                }
            }
            groups
                .entry(node.parent_id)
                .or_default()
                .push(node.position);
        }
        for positions in groups.values_mut() {
            positions.sort_unstable();
            if !positions.iter().copied().eq(0..positions.len()) {
                return Err(NotebookError::InvalidStructure(
                    "sibling positions are not contiguous",
                ));
            }
        }

        let next_id = match map.keys().next_back() {
            Some(max) => max
                .0
                .checked_add(1)
                .ok_or(NotebookError::InvalidStructure("node id out of range"))?,
            None => 0,
        };
        let notebook = Notebook {
            nodes: map,
            next_id,
        };

        // With every parent present, any node not reachable from a root
        // must be part of a cycle.
        if notebook.in_tree_order().len() != notebook.nodes.len() {
            return Err(NotebookError::InvalidStructure("parent cycle"));
        }
        Ok(notebook)
    }

    /// Like `from_nodes`, but the id counter is at least `next_id`, so ids
    /// that only exist elsewhere (Trash, checkpoints) are never reused.
    pub(crate) fn from_nodes_with_next_id(
        nodes: Vec<Node>,
        next_id: u64,
    ) -> Result<Self, NotebookError> {
        let mut notebook = Self::from_nodes(nodes)?;
        notebook.next_id = notebook.next_id.max(next_id);
        Ok(notebook)
    }

    /// The next id that will be handed out.
    pub(crate) fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Makes sure ids up to and including `max_id` are never handed out.
    pub(crate) fn reserve_ids_through(&mut self, max_id: u64) -> Result<(), NotebookError> {
        let needed = max_id
            .checked_add(1)
            .ok_or(NotebookError::InvalidStructure("node id out of range"))?;
        self.next_id = self.next_id.max(needed);
        Ok(())
    }

    /// A copy of every node (parents before children), for checkpoints.
    pub(crate) fn snapshot_nodes(&self) -> Vec<Node> {
        self.in_tree_order().into_iter().cloned().collect()
    }

    /// The node and all its descendants, root first, parents before
    /// children. The root keeps its original parent and position.
    pub(crate) fn subtree(&self, id: NodeId) -> Result<Vec<Node>, NotebookError> {
        let mut out = vec![self.require(id)?.clone()];
        let mut i = 0;
        while i < out.len() {
            let children = self.siblings(Some(out[i].id));
            out.extend(children.into_iter().cloned());
            i += 1;
        }
        Ok(out)
    }

    /// A notebook made of `nodes`, keeping this notebook's id counter so
    /// ids are never reused across the replacement.
    pub(crate) fn with_nodes_keeping_counter(
        &self,
        nodes: Vec<Node>,
    ) -> Result<Notebook, NotebookError> {
        Self::from_nodes_with_next_id(nodes, self.next_id)
    }

    /// Where `nodes` (a subtree, root first) would be restored: beneath its
    /// original parent if that is still active, at its original position
    /// clamped to the siblings available (a former root returns among the
    /// roots the same way); if the original parent is gone, at the end of the
    /// root list. Fails if any node id is already active.
    pub(crate) fn restore_placement(
        &self,
        nodes: &[Node],
    ) -> Result<(Option<NodeId>, usize), NotebookError> {
        if let Some(clash) = nodes.iter().find(|n| self.nodes.contains_key(&n.id)) {
            return Err(NotebookError::IdCollision(clash.id));
        }
        let root = nodes
            .first()
            .ok_or(NotebookError::InvalidStructure("empty subtree"))?;
        let roots = self.siblings(None).len();
        Ok(match root.parent_id {
            // It was a root: back to its old place among the roots.
            None => (None, root.position.min(roots)),
            Some(parent) if self.nodes.contains_key(&parent) => (
                Some(parent),
                root.position.min(self.siblings(Some(parent)).len()),
            ),
            // Its parent is gone: last root.
            Some(_) => (None, roots),
        })
    }

    /// Inserts a subtree at `restore_placement`, shifting later siblings
    /// down. Never replaces an active node. Returns where the root went.
    pub(crate) fn restore_subtree(
        &mut self,
        nodes: Vec<Node>,
    ) -> Result<(Option<NodeId>, usize), NotebookError> {
        let (parent, position) = self.restore_placement(&nodes)?;
        let max_id = nodes.iter().map(|n| n.id.0).max().unwrap_or(0);
        let next_id = max_id
            .checked_add(1)
            .ok_or(NotebookError::InvalidStructure("node id out of range"))?;

        let later: Vec<NodeId> = self
            .siblings(parent)
            .iter()
            .filter(|n| n.position >= position)
            .map(|n| n.id)
            .collect();
        for id in later {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.position += 1;
            }
        }
        for (i, mut node) in nodes.into_iter().enumerate() {
            if i == 0 {
                node.parent_id = parent;
                node.position = position;
            }
            self.nodes.insert(node.id, node);
        }
        self.next_id = self.next_id.max(next_id);
        Ok((parent, position))
    }

    /// All nodes, parents before their children, siblings in order.
    pub(crate) fn in_tree_order(&self) -> Vec<&Node> {
        let mut out = self.siblings(None);
        let mut i = 0;
        while i < out.len() {
            let children = self.siblings(Some(out[i].id));
            out.extend(children);
            i += 1;
        }
        out
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
