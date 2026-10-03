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

/// A validated, non-trivial move: where the node is now and where it ends up.
/// Positions are final sibling indexes, never Qt destination rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MovePlan {
    pub old_parent: Option<NodeId>,
    pub old_position: usize,
    pub new_parent: Option<NodeId>,
    pub new_position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookError {
    NodeNotFound(NodeId),
    /// Nodes handed to `Notebook::from_nodes` do not form a valid tree.
    InvalidStructure(&'static str),
    /// A node being restored has the same id as an active node.
    IdCollision(NodeId),
    /// A move would put a node beneath itself or one of its descendants.
    WouldCreateCycle,
}

impl fmt::Display for NotebookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotebookError::NodeNotFound(id) => write!(f, "node {} does not exist", id.0),
            NotebookError::InvalidStructure(why) => write!(f, "invalid notebook: {why}"),
            NotebookError::IdCollision(id) => write!(f, "node {} already exists", id.0),
            NotebookError::WouldCreateCycle => write!(f, "a node cannot be moved beneath itself"),
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

    /// Whether `node` is `ancestor` itself or lies somewhere beneath it.
    fn is_self_or_descendant(&self, node: NodeId, ancestor: NodeId) -> bool {
        let mut current = Some(node);
        // The tree is acyclic, but never loop forever on corrupt data.
        for _ in 0..=self.nodes.len() {
            match current {
                Some(id) if id == ancestor => return true,
                Some(id) => current = self.nodes.get(&id).and_then(|n| n.parent_id),
                None => return false,
            }
        }
        true
    }

    /// Whether `id` could be placed under `new_parent` (None = top level):
    /// both must exist and the move must not create a cycle.
    pub(crate) fn can_reparent(&self, id: NodeId, new_parent: Option<NodeId>) -> bool {
        self.nodes.contains_key(&id)
            && new_parent
                .is_none_or(|p| self.nodes.contains_key(&p) && !self.is_self_or_descendant(p, id))
    }

    /// Validates a move without changing anything.
    ///
    /// `new_position` is the node's desired *final* index among the children
    /// of `new_parent` once it has been moved. A position past the end is
    /// clamped to the last slot (there is no "below range": positions are
    /// unsigned). Returns `None` for a move that would leave the tree exactly
    /// as it is.
    pub(crate) fn plan_move(
        &self,
        id: NodeId,
        new_parent: Option<NodeId>,
        new_position: usize,
    ) -> Result<Option<MovePlan>, NotebookError> {
        let node = self.require(id)?;
        if let Some(parent) = new_parent {
            self.require(parent)?;
            if self.is_self_or_descendant(parent, id) {
                return Err(NotebookError::WouldCreateCycle);
            }
        }
        let others = self
            .siblings(new_parent)
            .iter()
            .filter(|n| n.id != id)
            .count();
        let final_position = new_position.min(others);
        if node.parent_id == new_parent && node.position == final_position {
            return Ok(None);
        }
        Ok(Some(MovePlan {
            old_parent: node.parent_id,
            old_position: node.position,
            new_parent,
            new_position: final_position,
        }))
    }

    /// Moves a node, with all its descendants, under `new_parent` (None =
    /// top level) at final sibling index `new_position`. Ids and contents are
    /// untouched and sibling positions stay contiguous. Returns whether the
    /// tree changed; on any error nothing changes.
    pub(crate) fn move_node(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        new_position: usize,
    ) -> Result<bool, NotebookError> {
        let Some(plan) = self.plan_move(id, new_parent, new_position)? else {
            return Ok(false);
        };
        let ids_under = |nb: &Notebook, parent: Option<NodeId>| -> Vec<NodeId> {
            nb.siblings(parent)
                .iter()
                .map(|n| n.id)
                .filter(|other| *other != id)
                .collect()
        };

        let mut old_group = ids_under(self, plan.old_parent);
        if plan.old_parent == plan.new_parent {
            old_group.insert(plan.new_position, id);
            self.renumber(&old_group);
        } else {
            self.renumber(&old_group);
            let mut new_group = ids_under(self, plan.new_parent);
            new_group.insert(plan.new_position, id);
            if let Some(node) = self.nodes.get_mut(&id) {
                node.parent_id = plan.new_parent;
            }
            self.renumber(&new_group);
        }
        Ok(true)
    }

    /// Sets positions 0.. in the given order.
    fn renumber(&mut self, order: &[NodeId]) {
        for (position, id) in order.iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(id) {
                node.position = position;
            }
        }
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

    // ---- move_node ----

    fn order(nb: &Notebook, parent: Option<NodeId>) -> Vec<String> {
        let nodes = match parent {
            None => nb.roots(),
            Some(p) => nb.children(p).unwrap(),
        };
        nodes.iter().map(|n| n.title().to_string()).collect()
    }

    /// Four roots A B C D, all with ids in creation order.
    fn four_roots() -> (Notebook, [NodeId; 4]) {
        let mut nb = Notebook::new();
        let ids = ["A", "B", "C", "D"].map(|t| nb.create_root(t));
        (nb, ids)
    }

    /// Every sibling group is numbered 0..n with no gaps or repeats.
    fn assert_contiguous(nb: &Notebook) {
        let mut groups: std::collections::BTreeMap<Option<NodeId>, Vec<usize>> = Default::default();
        for node in nb.snapshot_nodes() {
            groups
                .entry(node.parent_id())
                .or_default()
                .push(node.position());
        }
        for (parent, mut positions) in groups {
            positions.sort_unstable();
            assert!(
                positions.iter().copied().eq(0..positions.len()),
                "positions under {parent:?} are {positions:?}"
            );
        }
    }

    /// Projects
    /// ├── OmaTree
    /// │   └── Ideas
    /// │       └── Deep
    /// └── Threatwright
    /// Archive
    fn nested() -> (Notebook, [NodeId; 6]) {
        let mut nb = Notebook::new();
        let projects = nb.create_root("Projects");
        let archive = nb.create_root("Archive");
        let omatree = nb.create_child(projects, "OmaTree").unwrap();
        let threat = nb.create_child(projects, "Threatwright").unwrap();
        let ideas = nb.create_child(omatree, "Ideas").unwrap();
        let deep = nb.create_child(ideas, "Deep").unwrap();
        (nb, [projects, archive, omatree, threat, ideas, deep])
    }

    #[test]
    fn reorders_a_root_upward() {
        let (mut nb, [_, b, ..]) = four_roots();
        assert!(nb.move_node(b, None, 0).unwrap());
        assert_eq!(order(&nb, None), ["B", "A", "C", "D"]);
        assert_contiguous(&nb);
    }

    #[test]
    fn reorders_a_root_downward_to_its_final_position() {
        // The example from the ticket: move B to final position 3.
        let (mut nb, [_, b, ..]) = four_roots();
        assert!(nb.move_node(b, None, 3).unwrap());
        assert_eq!(order(&nb, None), ["A", "C", "D", "B"]);
        assert_eq!(nb.get(b).unwrap().position(), 3);
        assert_contiguous(&nb);
    }

    #[test]
    fn reorders_children_up_and_down() {
        let mut nb = Notebook::new();
        let p = nb.create_root("P");
        let ids = ["a", "b", "c", "d"].map(|t| nb.create_child(p, t).unwrap());
        assert!(nb.move_node(ids[2], Some(p), 0).unwrap());
        assert_eq!(order(&nb, Some(p)), ["c", "a", "b", "d"]);
        assert!(nb.move_node(ids[2], Some(p), 3).unwrap());
        assert_eq!(order(&nb, Some(p)), ["a", "b", "d", "c"]);
        assert_contiguous(&nb);
    }

    #[test]
    fn moves_a_root_beneath_another_root() {
        let (mut nb, [a, _, _, d]) = four_roots();
        assert!(nb.move_node(d, Some(a), 0).unwrap());
        assert_eq!(order(&nb, None), ["A", "B", "C"]);
        assert_eq!(order(&nb, Some(a)), ["D"]);
        assert_eq!(nb.get(d).unwrap().parent_id(), Some(a));
        assert_contiguous(&nb);
    }

    #[test]
    fn moves_a_child_between_parents() {
        let mut nb = Notebook::new();
        let p1 = nb.create_root("P1");
        let p2 = nb.create_root("P2");
        let x = nb.create_child(p1, "x").unwrap();
        nb.create_child(p1, "y").unwrap();
        nb.create_child(p2, "z").unwrap();
        assert!(nb.move_node(x, Some(p2), 0).unwrap());
        assert_eq!(order(&nb, Some(p1)), ["y"]);
        assert_eq!(order(&nb, Some(p2)), ["x", "z"]);
        assert_contiguous(&nb);
    }

    #[test]
    fn moves_a_child_to_the_root_level() {
        let (mut nb, [projects, _, omatree, ..]) = nested();
        assert!(nb.move_node(omatree, None, 1).unwrap());
        assert_eq!(order(&nb, None), ["Projects", "OmaTree", "Archive"]);
        assert_eq!(nb.get(omatree).unwrap().parent_id(), None);
        assert_eq!(order(&nb, Some(projects)), ["Threatwright"]);
        assert_contiguous(&nb);
    }

    #[test]
    fn moves_a_parent_with_its_whole_subtree_and_keeps_it_intact() {
        let (mut nb, [projects, archive, omatree, threat, ideas, deep]) = nested();
        let before: Vec<(NodeId, Option<NodeId>)> = nb
            .subtree(projects)
            .unwrap()
            .iter()
            .map(|n| (n.id(), n.parent_id()))
            .collect();
        assert!(nb.move_node(projects, Some(archive), 0).unwrap());
        assert_eq!(order(&nb, None), ["Archive"]);
        assert_eq!(order(&nb, Some(archive)), ["Projects"]);
        // Everything beneath it is still attached, exactly as before.
        let after = nb.subtree(projects).unwrap();
        assert_eq!(after.len(), 5);
        for (id, parent) in before.into_iter().skip(1) {
            assert_eq!(nb.get(id).unwrap().parent_id(), parent);
        }
        assert_eq!(nb.get(deep).unwrap().parent_id(), Some(ideas));
        assert_eq!(nb.get(ideas).unwrap().parent_id(), Some(omatree));
        assert_eq!(nb.get(threat).unwrap().parent_id(), Some(projects));
        assert_eq!(nb.get(projects).unwrap().parent_id(), Some(archive));
        assert_contiguous(&nb);
    }

    #[test]
    fn moves_keep_ids_titles_bodies_and_the_id_counter() {
        let (mut nb, [projects, archive, omatree, _, ideas, _]) = nested();
        nb.set_body(ideas, "ideas body").unwrap();
        let ids_before: Vec<NodeId> = nb.snapshot_nodes().iter().map(|n| n.id()).collect();
        let counter = nb.next_id();
        nb.move_node(omatree, Some(archive), 0).unwrap();
        nb.move_node(projects, None, 1).unwrap();
        let mut ids_after: Vec<NodeId> = nb.snapshot_nodes().iter().map(|n| n.id()).collect();
        let mut ids_before = ids_before;
        ids_before.sort();
        ids_after.sort();
        assert_eq!(ids_before, ids_after);
        assert_eq!(nb.next_id(), counter);
        assert_eq!(nb.get(ideas).unwrap().body(), "ideas body");
        assert_eq!(nb.get(ideas).unwrap().title(), "Ideas");
    }

    #[test]
    fn positions_stay_contiguous_after_every_kind_of_move() {
        let (mut nb, [projects, archive, omatree, threat, ..]) = nested();
        for (id, parent, pos) in [
            (threat, None, 0),
            (omatree, Some(archive), 5),
            (projects, Some(archive), 0),
            (threat, Some(archive), 1),
            (omatree, None, 0),
        ] {
            nb.move_node(id, parent, pos).unwrap();
            assert_contiguous(&nb);
        }
    }

    #[test]
    fn moving_beneath_itself_or_a_descendant_fails() {
        let (mut nb, [projects, _, omatree, _, ideas, deep]) = nested();
        let before = nb.snapshot_nodes();
        assert_eq!(
            nb.move_node(projects, Some(projects), 0),
            Err(NotebookError::WouldCreateCycle)
        );
        assert_eq!(
            nb.move_node(projects, Some(omatree), 0),
            Err(NotebookError::WouldCreateCycle)
        );
        assert_eq!(
            nb.move_node(projects, Some(deep), 0),
            Err(NotebookError::WouldCreateCycle)
        );
        assert_eq!(
            nb.move_node(omatree, Some(ideas), 0),
            Err(NotebookError::WouldCreateCycle)
        );
        assert_eq!(nb.snapshot_nodes(), before);
        assert!(!nb.can_reparent(projects, Some(deep)));
        assert!(nb.can_reparent(deep, Some(projects)));
        assert!(nb.can_reparent(deep, None));
    }

    #[test]
    fn a_missing_source_or_destination_fails() {
        let (mut nb, [a, ..]) = four_roots();
        let gone = nb.create_root("gone");
        nb.delete(gone).unwrap();
        let before = nb.snapshot_nodes();
        assert_eq!(
            nb.move_node(gone, None, 0),
            Err(NotebookError::NodeNotFound(gone))
        );
        assert_eq!(
            nb.move_node(a, Some(gone), 0),
            Err(NotebookError::NodeNotFound(gone))
        );
        assert_eq!(
            nb.plan_move(gone, None, 0),
            Err(NotebookError::NodeNotFound(gone))
        );
        assert!(!nb.can_reparent(gone, None));
        assert!(!nb.can_reparent(a, Some(gone)));
        assert_eq!(nb.snapshot_nodes(), before, "failures change nothing");
    }

    #[test]
    fn an_exact_no_op_is_not_a_change() {
        let (mut nb, [a, b, ..]) = four_roots();
        let before = nb.snapshot_nodes();
        assert_eq!(nb.move_node(b, None, 1), Ok(false), "row 1 to row 1");
        assert_eq!(nb.move_node(a, None, 0), Ok(false));
        assert_eq!(nb.plan_move(b, None, 1), Ok(None));
        // The last row to a position past the end is still where it is.
        let d = nb.roots()[3].id();
        assert_eq!(nb.move_node(d, None, 99), Ok(false));
        assert_eq!(nb.snapshot_nodes(), before);

        // Same parent, same effective position, further down the tree.
        let (mut nb, [projects, _, omatree, ..]) = nested();
        let before = nb.snapshot_nodes();
        assert_eq!(nb.move_node(omatree, Some(projects), 0), Ok(false));
        assert_eq!(nb.snapshot_nodes(), before);
    }

    #[test]
    fn positions_past_the_end_are_clamped_to_the_last_slot() {
        // One documented rule: the final position is clamped into range
        // (positions are unsigned, so there is no "below range").
        let (mut nb, [a, ..]) = four_roots();
        assert!(nb.move_node(a, None, 99).unwrap());
        assert_eq!(order(&nb, None), ["B", "C", "D", "A"]);

        let (mut nb, [p, archive, omatree, ..]) = nested();
        assert!(nb.move_node(omatree, Some(archive), 99).unwrap());
        assert_eq!(
            order(&nb, Some(archive)),
            ["OmaTree"],
            "appended to an empty parent"
        );
        assert!(nb.move_node(p, Some(archive), 99).unwrap());
        assert_eq!(
            order(&nb, Some(archive)),
            ["OmaTree", "Projects"],
            "cross-parent appends"
        );
        assert_contiguous(&nb);
    }

    #[test]
    fn position_zero_is_always_the_first_slot() {
        let (mut nb, [_, _, _, d]) = four_roots();
        assert!(nb.move_node(d, None, 0).unwrap());
        assert_eq!(order(&nb, None), ["D", "A", "B", "C"]);
    }
}
