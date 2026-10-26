//! The note tree as the egui shell shows it: every note in depth-first order with the little the
//! drawing needs (depth, whether it has children, branch-guide bits), plus the list of rows that
//! are currently visible (all ancestors open). It is rebuilt from `Document` only when the
//! structure or a title changes, and the visible list only when that or the expanded set changes;
//! a frame never walks the notebook. `Document` stays the single owner of hierarchy and of which
//! notes are open (`is_expanded`); nothing here is remembered across a notebook.

use omatree_core::document::Document;
use omatree_core::notebook::NodeId;
use std::collections::HashMap;

/// One note in the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    /// 0 for a top-level note.
    pub depth: usize,
    pub title: String,
    pub has_children: bool,
    /// The last of its siblings (its elbow stops at the middle).
    pub last: bool,
    /// Bit `k` set: the ancestor at depth `k + 1` has later siblings, so a vertical guide runs
    /// through this row in column `k` (columns `0..depth - 1`).
    pub guides: u32,
}

#[derive(Default)]
pub struct Tree {
    rows: Vec<Row>,
    index_of: HashMap<NodeId, usize>,
    /// Indexes into `rows` of the rows a user can see.
    visible: Vec<usize>,
}

impl Tree {
    pub fn build(doc: &Document) -> Tree {
        let nodes = doc.notebook().depth_first();
        let mut rows: Vec<Row> = Vec::with_capacity(nodes.len());
        let mut last_child: HashMap<Option<NodeId>, NodeId> = HashMap::new();
        let mut has_children: HashMap<NodeId, bool> = HashMap::new();
        for n in &nodes {
            last_child.insert(n.parent_id(), n.id());
            if let Some(p) = n.parent_id() {
                has_children.insert(p, true);
            }
        }
        let mut depth_of: HashMap<NodeId, usize> = HashMap::new();
        // `last_at[d]`: whether the most recent row at depth d is the last of its siblings.
        let mut last_at: Vec<bool> = Vec::new();
        for n in &nodes {
            let depth = n.parent_id().map_or(0, |p| depth_of[&p] + 1);
            depth_of.insert(n.id(), depth);
            let last = last_child.get(&n.parent_id()) == Some(&n.id());
            if last_at.len() <= depth {
                last_at.resize(depth + 1, true);
            }
            last_at[depth] = last;
            let mut guides = 0u32;
            for level in 0..depth.saturating_sub(1) {
                if level < 32 && !last_at[level + 1] {
                    guides |= 1 << level;
                }
            }
            rows.push(Row {
                id: n.id(),
                parent: n.parent_id(),
                depth,
                title: n.title().to_owned(),
                has_children: has_children.contains_key(&n.id()),
                last,
                guides,
            });
        }
        let index_of = rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let mut t = Tree {
            rows,
            index_of,
            visible: Vec::new(),
        };
        t.refresh_visible(doc);
        t
    }

    /// Recompute which rows are visible from the document's expanded set.
    pub fn refresh_visible(&mut self, doc: &Document) {
        self.visible.clear();
        let mut hidden_below: Option<usize> = None;
        for (i, r) in self.rows.iter().enumerate() {
            if let Some(d) = hidden_below {
                if r.depth > d {
                    continue;
                }
                hidden_below = None;
            }
            self.visible.push(i);
            if r.has_children && !doc.is_expanded(r.id) {
                hidden_below = Some(r.depth);
            }
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn len_visible(&self) -> usize {
        self.visible.len()
    }

    /// The `i`-th visible row.
    pub fn visible_row(&self, i: usize) -> &Row {
        &self.rows[self.visible[i]]
    }

    pub fn row(&self, id: NodeId) -> Option<&Row> {
        self.index_of.get(&id).map(|&i| &self.rows[i])
    }

    /// Position of `id` among the visible rows, if it is visible.
    pub fn visible_position(&self, id: NodeId) -> Option<usize> {
        let at = *self.index_of.get(&id)?;
        self.visible.binary_search(&at).ok()
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.index_of.contains_key(&id)
    }

    /// `id` and everything below it, in tree order.
    pub fn subtree_ids(&self, id: NodeId) -> Vec<NodeId> {
        let Some(&start) = self.index_of.get(&id) else {
            return Vec::new();
        };
        let depth = self.rows[start].depth;
        let mut out = vec![id];
        out.extend(
            self.rows[start + 1..]
                .iter()
                .take_while(|r| r.depth > depth)
                .map(|r| r.id),
        );
        out
    }

    /// The siblings of `id` (same parent), in order, including `id`.
    pub fn siblings(&self, id: NodeId) -> Vec<NodeId> {
        let Some(parent) = self.row(id).map(|r| r.parent) else {
            return Vec::new();
        };
        self.rows
            .iter()
            .filter(|r| r.parent == parent)
            .map(|r| r.id)
            .collect()
    }

    /// The nearest ancestor of `id` that is visible (or `id` itself when it is).
    pub fn nearest_visible(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if self.visible_position(c).is_some() {
                return Some(c);
            }
            cur = self.row(c).and_then(|r| r.parent);
        }
        None
    }
}
