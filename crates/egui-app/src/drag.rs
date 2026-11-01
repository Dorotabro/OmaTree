//! Where a dragged note would land (ticket E5b). Pure functions of the pointer's position in the
//! tree's content and of the `Tree`/`Document`: nothing here changes the notebook, and the verdict
//! on a move comes from `Document::plan_move`, so the indicator can only show what a drop would do.
//!
//! The zones follow the Qt app (`TreePane.qml`): upper quarter of a row = before it, lower quarter
//! = after it, the middle = as its child, space below the last row = end of the top level. Just
//! under an open parent "after" means its first child, as in Qt.

use super::tree::Tree;
use omatree_core::document::Document;
use omatree_core::notebook::NodeId;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Before,
    After,
    Child,
    /// Empty space below the rows: the end of the top level.
    End,
}

/// What releasing the pointer would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A real move to this parent and final sibling index.
    Move {
        parent: Option<NodeId>,
        position: usize,
    },
    /// The tree would stay exactly as it is: nothing happens, nothing is dirtied.
    NoOp,
    /// The notebook rejects it (cycle, or a title that already exists there).
    Refused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub kind: Kind,
    /// Visible row under the pointer (`len_visible()` for `End`).
    pub row: usize,
    /// The note under the pointer (none for `End`).
    pub over: Option<NodeId>,
    pub verdict: Verdict,
}

/// An in-flight drag. Nothing in the notebook has changed while this exists.
pub struct Drag {
    pub id: NodeId,
    /// The dragged note and everything below it (for the refusal check on hover-expansion).
    pub subtree: HashSet<NodeId>,
    pub target: Option<Target>,
    /// Escape was pressed: ignore the pointer until it is released.
    pub cancelled: bool,
    /// A collapsed note the pointer rests on and since when (egui time).
    pub hover: Option<(NodeId, f64)>,
    /// Notes opened by hovering; closed again when the drag ends.
    pub opened: Vec<NodeId>,
}

impl Drag {
    pub fn new(tree: &Tree, id: NodeId) -> Drag {
        Drag {
            id,
            subtree: tree.subtree_ids(id).into_iter().collect(),
            target: None,
            cancelled: false,
            hover: None,
            opened: Vec::new(),
        }
    }
}

/// The zone under the pointer: where the note would be inserted.
pub struct Zone {
    pub kind: Kind,
    pub row: usize,
    pub parent: Option<NodeId>,
    /// Sibling slot to insert before, counted with the dragged note in place (`usize::MAX`: last).
    pub slot: usize,
}

/// The zone under content-space height `y` (0 = top of the first row).
pub fn zone(tree: &Tree, doc: &Document, y: f32, row_h: f32) -> Option<Zone> {
    if y < 0.0 || tree.len_visible() == 0 {
        return None;
    }
    let n = tree.len_visible();
    let row = (y / row_h).floor() as usize;
    if row >= n {
        return Some(Zone {
            kind: Kind::End,
            row: n,
            parent: None,
            slot: usize::MAX,
        });
    }
    let r = tree.visible_row(row);
    let fraction = (y - row as f32 * row_h) / row_h;
    let position = doc.notebook().get(r.id).map_or(0, |node| node.position());
    let (kind, parent, slot) = if fraction < 0.25 {
        (Kind::Before, r.parent, position)
    } else if fraction > 0.75 {
        if r.has_children && doc.is_expanded(r.id) {
            (Kind::After, Some(r.id), 0)
        } else {
            (Kind::After, r.parent, position + 1)
        }
    } else {
        (Kind::Child, Some(r.id), usize::MAX)
    };
    Some(Zone {
        kind,
        row,
        parent,
        slot,
    })
}

/// The verdict of dropping `dragged` before sibling slot `slot` of `parent` (slots count with the
/// dragged note still in place, as in Qt; `usize::MAX` = last).
pub fn verdict(doc: &Document, dragged: NodeId, parent: Option<NodeId>, slot: usize) -> Verdict {
    // Removing the note from above the slot shifts the slot up by one; the notebook only ever
    // sees final positions.
    let position = match doc.notebook().get(dragged) {
        Some(node)
            if slot != usize::MAX && node.parent_id() == parent && node.position() < slot =>
        {
            slot - 1
        }
        _ => slot,
    };
    match doc.plan_move(dragged, parent, position) {
        Ok(Some(_)) => Verdict::Move { parent, position },
        Ok(None) => Verdict::NoOp,
        Err(_) => Verdict::Refused,
    }
}

/// The target under content-space height `y`, or none when the pointer is not over the rows.
///
/// `prev` is the target of the previous frame: while the pointer stays in the same zone the
/// verdict cannot have changed, so it is not asked of the notebook again.
pub fn locate(
    tree: &Tree,
    doc: &Document,
    dragged: NodeId,
    y: f32,
    row_h: f32,
    prev: Option<Target>,
) -> Option<Target> {
    let Zone {
        kind,
        row,
        parent,
        slot,
    } = zone(tree, doc, y, row_h)?;
    let over = (kind != Kind::End).then(|| tree.visible_row(row).id);
    if let Some(p) = prev {
        if (p.kind, p.row, p.over) == (kind, row, over) {
            return Some(p);
        }
    }
    Some(Target {
        kind,
        row,
        over,
        verdict: verdict(doc, dragged, parent, slot),
    })
}

/// Pixels per second to scroll while the pointer is within `edge` of the top or bottom of the
/// view (negative: towards the top). Zero elsewhere and outside the view.
pub fn scroll_speed(y: f32, top: f32, bottom: f32, edge: f32, max: f32) -> f32 {
    if y < top || y > bottom {
        return 0.0;
    }
    let edge = edge.min((bottom - top) / 2.0);
    if y < top + edge {
        -max * (1.0 - (y - top) / edge)
    } else if y > bottom - edge {
        max * (1.0 - (bottom - y) / edge)
    } else {
        0.0
    }
}
