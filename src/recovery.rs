//! Recovery state kept beside the active notebook: Trash for deleted
//! subtrees and checkpoints of the whole notebook. Pure Rust; none of this
//! lives inside the active `Notebook` tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::notebook::{Node, NodeId, Notebook, NotebookError};

/// Only the newest checkpoints are kept. Trash is never pruned.
pub const MAX_CHECKPOINTS: usize = 100;

/// The retained checkpoints together may hold at most this many bytes of
/// snapshot data (see `Checkpoint::size_bytes`), oldest dropped first. A single
/// checkpoint larger than this is still kept, alone: the newest recovery point
/// is never discarded for being big. Trash and the active notebook do not count.
pub const MAX_CHECKPOINT_BYTES: usize = 16 * 1024 * 1024;

/// What a stored node is counted as besides its title and body: ids, parent,
/// position and row bookkeeping.
const NODE_OVERHEAD_BYTES: usize = 64;

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// A deleted subtree. `nodes[0]` is the deleted root and keeps the parent id
/// and sibling position it had when it was deleted; the rest of the nodes
/// keep their own hierarchy and ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    deleted_at: i64,
    nodes: Vec<Node>,
}

impl TrashEntry {
    /// Fails unless `nodes` is a well-formed subtree with the root first.
    pub(crate) fn new(deleted_at: i64, mut nodes: Vec<Node>) -> Result<Self, NotebookError> {
        // Canonical order: root first, the rest by id, so an entry compares
        // equal after a trip through storage.
        if let Some(rest) = nodes.get_mut(1..) {
            rest.sort_by_key(|n| n.id());
        }
        let bad = |why| Err(NotebookError::InvalidStructure(why));
        let Some(root) = nodes.first() else {
            return bad("empty trash entry");
        };
        let ids: BTreeSet<NodeId> = nodes.iter().map(|n| n.id()).collect();
        if ids.len() != nodes.len() {
            return bad("duplicate node id in trash entry");
        }

        // Every non-root node must hang off another node of the entry, and
        // each sibling group must be numbered 0..n.
        let mut groups: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
        let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        for node in &nodes[1..] {
            match node.parent_id() {
                Some(parent) if ids.contains(&parent) => {
                    groups.entry(parent).or_default().push(node.position());
                    children.entry(parent).or_default().push(node.id());
                }
                _ => return bad("trash entry node has no parent in the entry"),
            }
        }
        for positions in groups.values_mut() {
            positions.sort_unstable();
            if !positions.iter().copied().eq(0..positions.len()) {
                return bad("trash entry sibling positions are not contiguous");
            }
        }

        // Everything must be reachable from the root (no detached cycles).
        let mut seen = 1;
        let mut stack = vec![root.id()];
        while let Some(id) = stack.pop() {
            for child in children.get(&id).into_iter().flatten() {
                seen += 1;
                stack.push(*child);
            }
        }
        if seen != nodes.len() {
            return bad("trash entry contains a cycle");
        }
        Ok(TrashEntry { deleted_at, nodes })
    }

    pub fn deleted_at(&self) -> i64 {
        self.deleted_at
    }

    pub fn title(&self) -> &str {
        self.nodes[0].title()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }
}

/// A copy of the whole active notebook at some moment, with the reason it
/// was taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    created_at: i64,
    reason: String,
    nodes: Vec<Node>,
    /// `snapshot_bytes` of the above, counted once.
    bytes: usize,
}

/// The bytes a snapshot is counted as: every title and body, the reason, and
/// a fixed overhead per node. Deterministic and cheap: nothing is serialized
/// and no file is asked.
fn snapshot_bytes(reason: &str, nodes: &[Node]) -> usize {
    nodes.iter().fold(reason.len(), |total, node| {
        total + node.title().len() + node.body().len() + NODE_OVERHEAD_BYTES
    })
}

impl Checkpoint {
    /// A checkpoint of a live notebook.
    pub(crate) fn of(notebook: &Notebook, reason: &str, created_at: i64) -> Self {
        let mut nodes = notebook.snapshot_nodes();
        nodes.sort_by_key(|n| n.id());
        let bytes = snapshot_bytes(reason, &nodes);
        Checkpoint {
            created_at,
            reason: reason.to_string(),
            nodes,
            bytes,
        }
    }

    /// A checkpoint read back from storage; fails unless it is a valid tree.
    pub(crate) fn from_stored(
        created_at: i64,
        reason: String,
        mut nodes: Vec<Node>,
    ) -> Result<Self, NotebookError> {
        Notebook::from_nodes(nodes.clone())?;
        nodes.sort_by_key(|n| n.id());
        let bytes = snapshot_bytes(&reason, &nodes);
        Ok(Checkpoint {
            created_at,
            reason,
            nodes,
            bytes,
        })
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// How much this snapshot counts against `MAX_CHECKPOINT_BYTES`.
    pub fn size_bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }
}

#[derive(Debug, Clone, Default)]
pub struct Recovery {
    trash: Vec<TrashEntry>,
    checkpoints: Vec<Checkpoint>,
}

impl Recovery {
    /// Both lists oldest first, as stored.
    pub(crate) fn from_parts(trash: Vec<TrashEntry>, checkpoints: Vec<Checkpoint>) -> Self {
        let mut recovery = Recovery {
            trash,
            checkpoints: Vec::new(),
        };
        for checkpoint in checkpoints {
            recovery.add_checkpoint(checkpoint);
        }
        recovery
    }

    /// Oldest first.
    pub fn trash(&self) -> &[TrashEntry] {
        &self.trash
    }

    /// Oldest first.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Adds a checkpoint, then drops the oldest ones until at most
    /// `MAX_CHECKPOINTS` remain and, unless only the newest is left, their
    /// sizes add up to at most `MAX_CHECKPOINT_BYTES`.
    pub(crate) fn add_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoints.push(checkpoint);
        while self.checkpoints.len() > MAX_CHECKPOINTS {
            self.checkpoints.remove(0);
        }
        let mut total = self.checkpoint_bytes();
        while total > MAX_CHECKPOINT_BYTES && self.checkpoints.len() > 1 {
            total -= self.checkpoints.remove(0).size_bytes();
        }
    }

    /// The snapshot data currently retained, as counted for the budget.
    pub fn checkpoint_bytes(&self) -> usize {
        self.checkpoints.iter().map(Checkpoint::size_bytes).sum()
    }

    pub(crate) fn push_trash(&mut self, entry: TrashEntry) {
        self.trash.push(entry);
    }

    pub(crate) fn take_trash(&mut self, index: usize) -> Option<TrashEntry> {
        (index < self.trash.len()).then(|| self.trash.remove(index))
    }

    /// The highest node id appearing anywhere in the recovery state.
    pub(crate) fn max_node_id(&self) -> Option<u64> {
        let trash = self.trash.iter().flat_map(|e| e.nodes.iter());
        let checkpoints = self.checkpoints.iter().flat_map(|c| c.nodes.iter());
        trash.chain(checkpoints).map(|n| n.id().get()).max()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    /// No Trash entry or checkpoint at that position.
    NoSuchEntry,
    /// Restoring would collide with an active node id.
    IdCollision,
    /// A sibling at the destination already has the same title.
    TitleConflict,
    /// The stored data cannot form a valid notebook.
    Invalid,
}

impl From<NotebookError> for RecoveryError {
    fn from(e: NotebookError) -> Self {
        match e {
            NotebookError::IdCollision(_) => RecoveryError::IdCollision,
            NotebookError::TitleConflict => RecoveryError::TitleConflict,
            _ => RecoveryError::Invalid,
        }
    }
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl RecoveryError {
    /// Short human-readable text.
    pub fn message(&self) -> &'static str {
        match self {
            RecoveryError::NoSuchEntry => "That item is no longer available.",
            RecoveryError::IdCollision => {
                "Some of these notes already exist in the notebook, so they were not restored."
            }
            RecoveryError::TitleConflict => {
                "A note with the same name already exists where this would be restored. \
                 Rename or move it first."
            }
            RecoveryError::Invalid => "That item could not be restored.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, parent: Option<u64>, position: usize, title: &str) -> Node {
        Node::from_parts(
            NodeId::from_raw(id),
            parent.map(NodeId::from_raw),
            position,
            title.to_string(),
            String::new(),
        )
    }

    #[test]
    fn trash_entry_accepts_a_well_formed_subtree() {
        // The root's own parent (99) is outside the entry, which is fine.
        let nodes = vec![
            node(5, Some(99), 3, "root"),
            node(6, Some(5), 0, "a"),
            node(7, Some(5), 1, "b"),
            node(8, Some(6), 0, "a1"),
        ];
        let entry = TrashEntry::new(10, nodes).unwrap();
        assert_eq!(entry.title(), "root");
        assert_eq!(entry.node_count(), 4);
        assert_eq!(entry.root().parent_id(), Some(NodeId::from_raw(99)));
        assert_eq!(entry.root().position(), 3);
    }

    #[test]
    fn trash_entry_rejects_malformed_subtrees() {
        assert!(TrashEntry::new(0, vec![]).is_err());
        // duplicate ids
        assert!(TrashEntry::new(0, vec![node(1, None, 0, "a"), node(1, Some(1), 0, "b")]).is_err());
        // a node whose parent is not in the entry
        assert!(TrashEntry::new(0, vec![node(1, None, 0, "a"), node(2, Some(9), 0, "b")]).is_err());
        // sibling positions with a gap
        assert!(TrashEntry::new(0, vec![node(1, None, 0, "a"), node(2, Some(1), 1, "b")]).is_err());
        // a detached cycle
        assert!(TrashEntry::new(
            0,
            vec![
                node(1, None, 0, "root"),
                node(2, Some(3), 0, "x"),
                node(3, Some(2), 0, "y"),
            ]
        )
        .is_err());
    }

    #[test]
    fn only_the_newest_checkpoints_are_kept() {
        let nb = Notebook::new();
        let mut recovery = Recovery::default();
        for i in 0..(MAX_CHECKPOINTS + 5) {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i as i64));
        }
        assert_eq!(recovery.checkpoints().len(), MAX_CHECKPOINTS);
        assert_eq!(recovery.checkpoints()[0].reason(), "c5");
        assert_eq!(
            recovery.checkpoints().last().unwrap().reason(),
            format!("c{}", MAX_CHECKPOINTS + 4)
        );
    }

    #[test]
    fn trash_is_never_pruned_by_the_checkpoint_limit() {
        let mut recovery = Recovery::default();
        for i in 0..(MAX_CHECKPOINTS + 20) {
            recovery
                .push_trash(TrashEntry::new(i as i64, vec![node(i as u64, None, 0, "t")]).unwrap());
        }
        assert_eq!(recovery.trash().len(), MAX_CHECKPOINTS + 20);
    }

    #[test]
    fn max_node_id_covers_trash_and_checkpoints() {
        let mut recovery = Recovery::default();
        assert_eq!(recovery.max_node_id(), None);
        recovery.push_trash(TrashEntry::new(0, vec![node(40, None, 0, "t")]).unwrap());
        let mut nb = Notebook::new();
        nb.create_root("x").unwrap();
        recovery.add_checkpoint(Checkpoint::of(&nb, "c", 0));
        assert_eq!(recovery.max_node_id(), Some(40));
    }

    // ---- the byte budget ----

    const MIB: usize = 1024 * 1024;

    /// A notebook of one note whose body is `bytes` long.
    fn notebook_of(bytes: usize) -> Notebook {
        let mut nb = Notebook::new();
        let id = nb.create_root("n").unwrap();
        nb.set_body(id, &"x".repeat(bytes)).unwrap();
        nb
    }

    fn reasons(recovery: &Recovery) -> Vec<String> {
        recovery
            .checkpoints()
            .iter()
            .map(|c| c.reason().to_string())
            .collect()
    }

    #[test]
    fn a_checkpoints_size_counts_titles_bodies_reason_and_node_overhead() {
        let mut nb = Notebook::new();
        let a = nb.create_root("title").unwrap(); // 5 bytes
        nb.set_body(a, "body!").unwrap(); // 5 bytes
        nb.create_root("é").unwrap(); // 2 bytes in UTF-8
        let checkpoint = Checkpoint::of(&nb, "why", 0); // 3 bytes
        assert_eq!(
            checkpoint.size_bytes(),
            3 + (5 + 5 + NODE_OVERHEAD_BYTES) + (2 + NODE_OVERHEAD_BYTES)
        );
    }

    #[test]
    fn checkpoints_within_the_budget_are_all_kept() {
        let nb = notebook_of(MIB);
        let mut recovery = Recovery::default();
        for i in 0..10 {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i));
        }
        assert_eq!(recovery.checkpoints().len(), 10);
        assert!(recovery.checkpoint_bytes() <= MAX_CHECKPOINT_BYTES);
    }

    #[test]
    fn the_budget_drops_the_oldest_first() {
        // 3 MiB each: five fit in 16 MiB, a sixth pushes the oldest out.
        let nb = notebook_of(3 * MIB);
        let mut recovery = Recovery::default();
        for i in 0..8 {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i));
            assert!(recovery.checkpoint_bytes() <= MAX_CHECKPOINT_BYTES);
        }
        assert_eq!(reasons(&recovery), ["c3", "c4", "c5", "c6", "c7"]);
    }

    #[test]
    fn the_newest_checkpoint_is_always_kept() {
        let nb = notebook_of(3 * MIB);
        let mut recovery = Recovery::default();
        for i in 0..20 {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i));
            assert_eq!(
                recovery.checkpoints().last().unwrap().reason(),
                format!("c{i}")
            );
        }
    }

    #[test]
    fn one_oversized_newest_checkpoint_is_kept_alone() {
        let small = notebook_of(MIB);
        let huge = notebook_of(MAX_CHECKPOINT_BYTES + MIB);
        let mut recovery = Recovery::default();
        for i in 0..4 {
            recovery.add_checkpoint(Checkpoint::of(&small, &format!("small{i}"), i));
        }
        recovery.add_checkpoint(Checkpoint::of(&huge, "huge", 9));
        assert_eq!(reasons(&recovery), ["huge"]);
        assert!(
            recovery.checkpoint_bytes() > MAX_CHECKPOINT_BYTES,
            "the allowed exception"
        );
    }

    #[test]
    fn a_small_checkpoint_after_an_oversized_one_pushes_it_out() {
        let small = notebook_of(MIB);
        let huge = notebook_of(MAX_CHECKPOINT_BYTES + MIB);
        let mut recovery = Recovery::default();
        recovery.add_checkpoint(Checkpoint::of(&huge, "huge", 0));
        recovery.add_checkpoint(Checkpoint::of(&small, "small", 1));
        assert_eq!(reasons(&recovery), ["small"]);
        recovery.add_checkpoint(Checkpoint::of(&small, "small2", 2));
        assert_eq!(reasons(&recovery), ["small", "small2"]);
    }

    #[test]
    fn the_count_cap_still_applies_below_the_byte_budget() {
        let nb = Notebook::new();
        let mut recovery = Recovery::default();
        for i in 0..(MAX_CHECKPOINTS + 7) {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i as i64));
        }
        assert_eq!(recovery.checkpoints().len(), MAX_CHECKPOINTS);
        assert!(recovery.checkpoint_bytes() < MAX_CHECKPOINT_BYTES);
    }

    #[test]
    fn trash_is_not_counted_and_not_pruned_by_the_budget() {
        let nb = notebook_of(3 * MIB);
        let mut recovery = Recovery::default();
        recovery.push_trash(TrashEntry::new(0, vec![node(1, None, 0, "t")]).unwrap());
        for i in 0..10 {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("c{i}"), i));
        }
        assert_eq!(recovery.trash().len(), 1);
        assert_eq!(recovery.checkpoints().len(), 5);
    }

    #[test]
    fn loading_applies_the_budget_in_memory() {
        let nb = notebook_of(3 * MIB);
        let many: Vec<Checkpoint> = (0..9)
            .map(|i| Checkpoint::of(&nb, &format!("c{i}"), i))
            .collect();
        let recovery = Recovery::from_parts(Vec::new(), many);
        assert_eq!(reasons(&recovery), ["c4", "c5", "c6", "c7", "c8"]);
    }

    /// The audit's scenario in miniature: a notebook with real bodies and many
    /// structural changes. Retained snapshot data stays within the rule, so
    /// the file can no longer grow towards a hundred copies.
    #[test]
    fn many_structural_checkpoints_stay_bounded() {
        let mut nb = Notebook::new();
        for i in 0..100 {
            let id = nb.create_root(&format!("note {i}")).unwrap();
            nb.set_body(id, &"b".repeat(100_000)).unwrap(); // ~10 MB notebook
        }
        let one = Checkpoint::of(&nb, "x", 0).size_bytes();
        let mut recovery = Recovery::default();
        for i in 0..100 {
            recovery.add_checkpoint(Checkpoint::of(&nb, &format!("move {i}"), i));
            assert!(recovery.checkpoint_bytes() <= MAX_CHECKPOINT_BYTES.max(one));
        }
        assert!(recovery.checkpoints().len() <= MAX_CHECKPOINT_BYTES / one + 1);
        assert_eq!(recovery.checkpoints().last().unwrap().reason(), "move 99");
    }
}
