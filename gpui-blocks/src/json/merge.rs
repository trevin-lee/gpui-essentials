//! Merging two edits of one document, and the block operations underneath.
//!
//! An app that saves whole documents against a base revision gets a refusal when someone saved
//! first; it then merges the two with [`merge`] and saves the result. [`diff`] and [`apply`] are
//! the same changes as a list of block operations, for sending edits rather than pages.
//!
//! Everything here works on [`Document`]s and their block ids, with no editor.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::{Block, Document, Kind};

/// Which edit of the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Ours,
    Theirs,
}

/// A change the merge couldn't apply as both sides made it. Ours is kept in each case; the
/// conflict says what of theirs was set aside, or what was kept against a deletion, so nothing
/// is lost without a report.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Conflict {
    /// Both changed the block's content in ways that overlap. Ours is kept; `theirs` is
    /// their version of the block (without its children).
    Content { id: String, theirs: Kind },
    /// One side deleted the block and the other changed it: the change is kept.
    DeletedChanged { id: String, deleted_by: Side },
    /// One side deleted the block, and the other put a block inside it: it's kept for that.
    Restored { id: String, deleted_by: Side },
    /// Both moved the block to different places, or their moves would nest it in itself. Ours
    /// is kept; `parent` and `after` are where theirs put it.
    Moved {
        id: String,
        parent: Option<String>,
        after: Option<String>,
    },
    /// Both changed the front matter differently. Ours is kept.
    FrontMatter { theirs: String },
}

/// One change to a document's blocks. A list of them, from [`diff`], turns one document into
/// another with [`apply`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// A new block, without children, in `parent` (`None` for the top level) after the block
    /// `after` (`None` for first).
    Insert {
        id: String,
        #[serde(flatten)]
        kind: Kind,
        parent: Option<String>,
        after: Option<String>,
    },
    /// The block, with everything in it, to `parent` after `after`.
    Move {
        id: String,
        parent: Option<String>,
        after: Option<String>,
    },
    /// The block's content (not its children) becomes `kind`.
    Update {
        id: String,
        #[serde(flatten)]
        kind: Kind,
    },
    /// The block and everything in it go.
    Delete { id: String },
    /// The front matter becomes `front_matter`.
    FrontMatter { front_matter: String },
}

/// Why operations couldn't be applied.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyError {
    /// No block with this id.
    Missing(String),
    /// A block with this id is already there.
    Exists(String),
    /// `after` isn't a child of the parent given.
    NotSibling(String),
    /// The move would put a block inside itself.
    Cycle(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(id) => write!(f, "no block {id:?}"),
            Self::Exists(id) => write!(f, "block {id:?} is already there"),
            Self::NotSibling(id) => write!(f, "{id:?} isn't in that parent"),
            Self::Cycle(id) => write!(f, "{id:?} can't go inside itself"),
        }
    }
}

impl std::error::Error for ApplyError {}

// A document as a flat tree. The top level is the parent "", which no block id can be.

const ROOT: &str = "";

#[derive(Clone, Debug, PartialEq)]
struct Node {
    kind: Kind,
    parent: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Tree {
    nodes: HashMap<String, Node>,
    children: HashMap<String, Vec<String>>,
}

impl Tree {
    fn of(doc: &Document) -> Self {
        let mut tree = Tree::default();
        tree.children.insert(ROOT.into(), Vec::new());
        fn add(tree: &mut Tree, parent: &str, blocks: &[Block]) {
            for b in blocks {
                // A repeated id keeps its first place.
                if tree.nodes.contains_key(&b.id) || b.id.is_empty() {
                    continue;
                }
                tree.nodes.insert(
                    b.id.clone(),
                    Node {
                        kind: b.kind.clone(),
                        parent: parent.into(),
                    },
                );
                tree.children
                    .entry(parent.into())
                    .or_default()
                    .push(b.id.clone());
                tree.children.entry(b.id.clone()).or_default();
                add(tree, &b.id, &b.children);
            }
        }
        add(&mut tree, ROOT, &doc.blocks);
        tree
    }

    fn blocks(&self, parent: &str) -> Vec<Block> {
        self.children
            .get(parent)
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let node = self.nodes.get(id)?;
                Some(Block {
                    id: id.clone(),
                    kind: node.kind.clone(),
                    children: self.blocks(id),
                })
            })
            .collect()
    }

    fn document(&self, front_matter: String) -> Document {
        Document {
            format: super::FORMAT.into(),
            version: super::VERSION,
            front_matter,
            blocks: self.blocks(ROOT),
        }
    }

    fn siblings(&self, parent: &str) -> &[String] {
        self.children.get(parent).map_or(&[], Vec::as_slice)
    }

    /// The block before `id` among its siblings.
    fn after(&self, id: &str) -> Option<String> {
        let parent = &self.nodes.get(id)?.parent;
        let list = self.siblings(parent);
        let at = list.iter().position(|x| x == id)?;
        at.checked_sub(1).map(|i| list[i].clone())
    }

    fn contains_in(&self, ancestor: &str, id: &str) -> bool {
        let mut at = id;
        while let Some(node) = self.nodes.get(at) {
            if node.parent == ancestor {
                return true;
            }
            at = &node.parent;
        }
        ancestor == ROOT
    }

    fn detach(&mut self, id: &str) {
        if let Some(node) = self.nodes.get(id) {
            let parent = node.parent.clone();
            if let Some(list) = self.children.get_mut(&parent) {
                list.retain(|x| x != id);
            }
        }
    }

    fn place(&mut self, id: &str, parent: &str, after: Option<&str>) -> Result<(), ApplyError> {
        let list = self
            .children
            .get_mut(parent)
            .ok_or_else(|| ApplyError::Missing(parent.into()))?;
        let at = match after {
            None => 0,
            Some(a) => {
                list.iter()
                    .position(|x| x == a)
                    .ok_or_else(|| ApplyError::NotSibling(a.into()))?
                    + 1
            }
        };
        list.insert(at, id.into());
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = parent.into();
        }
        Ok(())
    }

    fn remove(&mut self, id: &str) {
        self.detach(id);
        let mut stack = vec![id.to_string()];
        while let Some(at) = stack.pop() {
            self.nodes.remove(&at);
            if let Some(kids) = self.children.remove(&at) {
                stack.extend(kids);
            }
        }
    }

    /// Ids in document order.
    fn order(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<&String> = self.siblings(ROOT).iter().rev().collect();
        while let Some(id) = stack.pop() {
            out.push(id.clone());
            stack.extend(self.siblings(id).iter().rev());
        }
        out
    }
}

fn parent_opt(parent: &str) -> Option<String> {
    (parent != ROOT).then(|| parent.to_string())
}

fn parent_key(parent: &Option<String>) -> &str {
    parent.as_deref().unwrap_or(ROOT)
}

// diff and apply.

/// The operations that turn `a` into `b`: [`apply`]`(a, diff(a, b))` is `b`. Blocks are matched
/// by id.
pub fn diff(a: &Document, b: &Document) -> Vec<Op> {
    let (mut work, target) = (Tree::of(a), Tree::of(b));
    let mut ops = Vec::new();
    if a.front_matter != b.front_matter {
        ops.push(Op::FrontMatter {
            front_matter: b.front_matter.clone(),
        });
    }
    // New blocks first, parents before children, each at the end of its parent for now.
    for id in target.order() {
        if work.nodes.contains_key(&id) {
            continue;
        }
        let node = &target.nodes[&id];
        let after = work.siblings(&node.parent).last().cloned();
        work.nodes.insert(id.clone(), node.clone());
        work.children.entry(id.clone()).or_default();
        let _ = work.place(&id, &node.parent, after.as_deref());
        ops.push(Op::Insert {
            id: id.clone(),
            kind: node.kind.clone(),
            parent: parent_opt(&node.parent),
            after,
        });
    }
    // Then every parent's children into order, moving as few as can be.
    for parent in std::iter::once(ROOT.to_string()).chain(target.order()) {
        let wanted = target.siblings(&parent).to_vec();
        let here: Vec<String> = work
            .siblings(&parent)
            .iter()
            .filter(|id| wanted.contains(id))
            .cloned()
            .collect();
        let stay = lcs(&here, &wanted);
        let mut prev: Option<String> = None;
        for id in &wanted {
            if !stay.contains(id) || work.nodes[id].parent != parent {
                work.detach(id);
                let _ = work.place(id, &parent, prev.as_deref());
                ops.push(Op::Move {
                    id: id.clone(),
                    parent: parent_opt(&parent),
                    after: prev.clone(),
                });
            }
            prev = Some(id.clone());
        }
    }
    for id in target.order() {
        if work.nodes[&id].kind != target.nodes[&id].kind {
            ops.push(Op::Update {
                id: id.clone(),
                kind: target.nodes[&id].kind.clone(),
            });
        }
    }
    // Last, the blocks that went; by now nothing that stays is inside them.
    for id in work.order() {
        if target.nodes.contains_key(&id) || !work.nodes.contains_key(&id) {
            continue;
        }
        let parent = work.nodes[&id].parent.clone();
        if parent == ROOT || target.nodes.contains_key(&parent) {
            work.remove(&id);
            ops.push(Op::Delete { id });
        }
    }
    ops
}

/// The ids of a longest common subsequence of `a` and `b`, which can stay where they are. Ids
/// are unique in each list, so it's a longest increasing run of `a`'s places in `b`: O(n log n),
/// however long the lists.
fn lcs(a: &[String], b: &[String]) -> HashSet<String> {
    let place: HashMap<&str, usize> = b.iter().enumerate().map(|(i, x)| (x.as_str(), i)).collect();
    let seq: Vec<(usize, &String)> = a
        .iter()
        .filter_map(|x| place.get(x.as_str()).map(|p| (*p, x)))
        .collect();
    // `tails[k]`: the entry of `seq` ending the increasing run of length `k + 1` with the
    // smallest place; `prev`: each entry's predecessor in its run.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, (p, _)) in seq.iter().enumerate() {
        let k = tails.partition_point(|t| seq[*t].0 < *p);
        if k > 0 {
            prev[i] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = HashSet::new();
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.insert(seq[i].1.clone());
        at = prev[i];
    }
    out
}

/// `ops` applied to `doc` in order. On an error nothing is returned.
pub fn apply(doc: &Document, ops: &[Op]) -> Result<Document, ApplyError> {
    let mut tree = Tree::of(doc);
    let mut front_matter = doc.front_matter.clone();
    for op in ops {
        match op {
            Op::Insert {
                id,
                kind,
                parent,
                after,
            } => {
                if id.is_empty() || tree.nodes.contains_key(id) {
                    return Err(ApplyError::Exists(id.clone()));
                }
                let parent = parent_key(parent);
                if parent != ROOT && !tree.nodes.contains_key(parent) {
                    return Err(ApplyError::Missing(parent.into()));
                }
                tree.nodes.insert(
                    id.clone(),
                    Node {
                        kind: kind.clone(),
                        parent: parent.into(),
                    },
                );
                tree.children.insert(id.clone(), Vec::new());
                if let Err(e) = tree.place(id, parent, after.as_deref()) {
                    tree.nodes.remove(id);
                    return Err(e);
                }
            }
            Op::Move { id, parent, after } => {
                if !tree.nodes.contains_key(id) {
                    return Err(ApplyError::Missing(id.clone()));
                }
                let parent = parent_key(parent);
                if parent != ROOT && !tree.nodes.contains_key(parent) {
                    return Err(ApplyError::Missing(parent.into()));
                }
                if parent == id || (parent != ROOT && tree.contains_in(id, parent)) {
                    return Err(ApplyError::Cycle(id.clone()));
                }
                if after.as_deref() == Some(id.as_str()) {
                    return Err(ApplyError::NotSibling(id.clone()));
                }
                let old = (tree.nodes[id].parent.clone(), tree.after(id));
                tree.detach(id);
                if let Err(e) = tree.place(id, parent, after.as_deref()) {
                    let _ = tree.place(id, &old.0, old.1.as_deref());
                    return Err(e);
                }
            }
            Op::Update { id, kind } => {
                tree.nodes
                    .get_mut(id)
                    .ok_or_else(|| ApplyError::Missing(id.clone()))?
                    .kind = kind.clone();
            }
            Op::Delete { id } => {
                if !tree.nodes.contains_key(id) {
                    return Err(ApplyError::Missing(id.clone()));
                }
                tree.remove(id);
            }
            Op::FrontMatter { front_matter: f } => front_matter = f.clone(),
        }
    }
    Ok(tree.document(front_matter))
}

// The merge.

/// A three-way merge of two edits, `ours` and `theirs`, of `base`, by block id.
///
/// - A change one side made where the other left the block alone applies: content edits,
///   moves, inserts (which keep their place among their neighbours) and deletions.
/// - Both changed a block's content: text (of a paragraph, heading, cell or code block) merges
///   when their edits don't overlap; otherwise ours is kept and theirs is a
///   [`Conflict::Content`].
/// - One deleted a block the other changed: the change is kept ([`Conflict::DeletedChanged`]).
///   One deleted a block the other put something inside: it's kept for that
///   ([`Conflict::Restored`]).
/// - Both moved a block to different places: ours is kept ([`Conflict::Moved`]).
///
/// Merging with an unchanged side gives the other side, and apart from which side wins a
/// conflict, the order of `ours` and `theirs` doesn't change the result.
pub fn merge(base: &Document, ours: &Document, theirs: &Document) -> (Document, Vec<Conflict>) {
    let (b, o, t) = (Tree::of(base), Tree::of(ours), Tree::of(theirs));
    let mut conflicts = Vec::new();

    let front_matter = match (
        ours.front_matter == base.front_matter,
        theirs.front_matter == base.front_matter,
    ) {
        (true, _) => theirs.front_matter.clone(),
        (false, true) => ours.front_matter.clone(),
        (false, false) => {
            if ours.front_matter != theirs.front_matter {
                conflicts.push(Conflict::FrontMatter {
                    theirs: theirs.front_matter.clone(),
                });
            }
            ours.front_matter.clone()
        }
    };

    // Which blocks stay, with what content.
    let mut ids: Vec<String> = o.order();
    ids.extend(t.order().into_iter().filter(|id| !o.nodes.contains_key(id)));
    let mut kinds: HashMap<String, Kind> = HashMap::new();
    let mut deleted_by: HashMap<String, Side> = HashMap::new();
    for id in &ids {
        let (bk, ok, tk) = (
            b.nodes.get(id).map(|n| &n.kind),
            o.nodes.get(id).map(|n| &n.kind),
            t.nodes.get(id).map(|n| &n.kind),
        );
        let kind = match (bk, ok, tk) {
            (Some(bk), Some(ok), Some(tk)) => merge_kind(id, bk, ok, tk, &mut conflicts),
            (Some(bk), Some(ok), None) => {
                deleted_by.insert(id.clone(), Side::Theirs);
                if ok == bk {
                    continue;
                }
                conflicts.push(Conflict::DeletedChanged {
                    id: id.clone(),
                    deleted_by: Side::Theirs,
                });
                ok.clone()
            }
            (Some(bk), None, Some(tk)) => {
                deleted_by.insert(id.clone(), Side::Ours);
                if tk == bk {
                    continue;
                }
                conflicts.push(Conflict::DeletedChanged {
                    id: id.clone(),
                    deleted_by: Side::Ours,
                });
                tk.clone()
            }
            (None, Some(ok), Some(tk)) => {
                if ok != tk {
                    conflicts.push(Conflict::Content {
                        id: id.clone(),
                        theirs: tk.clone(),
                    });
                }
                ok.clone()
            }
            (None, Some(ok), None) => ok.clone(),
            (None, None, Some(tk)) => tk.clone(),
            _ => continue,
        };
        kinds.insert(id.clone(), kind);
    }

    // Where each block goes: the side that moved it, else where it was.
    let places = Places::new(&b);
    let moved = |side: &Tree, id: &str| -> bool {
        let (Some(bn), Some(sn)) = (b.nodes.get(id), side.nodes.get(id)) else {
            return false;
        };
        bn.parent != sn.parent || !places.same_order(side, id)
    };
    let mut source: HashMap<String, From> = HashMap::new();
    for id in kinds.keys() {
        let side = match (o.nodes.contains_key(id), t.nodes.contains_key(id)) {
            (true, false) => From::Ours,
            (false, true) => From::Theirs,
            _ => {
                let (mo, mt) = (moved(&o, id), moved(&t, id));
                if mo && mt {
                    if o.nodes[id].parent != t.nodes[id].parent || o.after(id) != t.after(id) {
                        conflicts.push(Conflict::Moved {
                            id: id.clone(),
                            parent: parent_opt(&t.nodes[id].parent),
                            after: t.after(id),
                        });
                    }
                    From::Ours
                } else if mt {
                    From::Theirs
                } else {
                    From::Ours
                }
            }
        };
        source.insert(id.clone(), side);
    }
    let parent_in = |from: From, id: &str| -> String {
        match from {
            From::Ours => &o,
            From::Theirs => &t,
            From::Base => &b,
        }
        .nodes
        .get(id)
        .map_or_else(|| ROOT.to_string(), |n| n.parent.clone())
    };

    // Blocks kept; a deleted block something stays inside is kept too.
    let mut parent_of: HashMap<String, String>;
    loop {
        parent_of = source
            .iter()
            .map(|(id, from)| (id.clone(), parent_in(*from, id)))
            .collect();
        // Moves on two sides can make a loop; theirs gives way.
        let ids_now: Vec<String> = source.keys().cloned().collect();
        for id in ids_now {
            let mut seen = HashSet::new();
            let mut at = id.clone();
            while let Some(p) = parent_of.get(&at).cloned() {
                if p == id {
                    if source[&id] == From::Theirs && o.nodes.contains_key(&id) {
                        conflicts.push(Conflict::Moved {
                            id: id.clone(),
                            parent: parent_opt(&t.nodes[&id].parent),
                            after: t.after(&id),
                        });
                        source.insert(id.clone(), From::Ours);
                        parent_of.insert(id.clone(), parent_in(From::Ours, &id));
                    }
                    break;
                }
                if !seen.insert(p.clone()) {
                    break;
                }
                at = p;
            }
        }
        let missing: HashSet<String> = parent_of
            .values()
            .filter(|p| *p != ROOT && !source.contains_key(*p))
            .cloned()
            .collect();
        if missing.is_empty() {
            break;
        }
        for p in missing {
            // Deleted by one side, which the other still has; else back from the base.
            let from = if o.nodes.contains_key(&p) {
                From::Ours
            } else if t.nodes.contains_key(&p) {
                From::Theirs
            } else {
                From::Base
            };
            let node = match from {
                From::Ours => &o.nodes[&p],
                From::Theirs => &t.nodes[&p],
                From::Base => match b.nodes.get(&p) {
                    Some(n) => n,
                    None => continue,
                },
            };
            kinds.insert(p.clone(), node.kind.clone());
            source.insert(p.clone(), from);
            let by = deleted_by.get(&p).copied().unwrap_or(Side::Ours);
            conflicts.push(Conflict::Restored {
                id: p.clone(),
                deleted_by: by,
            });
        }
    }

    // Each parent's children: ours' order, then theirs' blocks after their neighbours there.
    let mut tree = Tree::default();
    for (id, parent) in &parent_of {
        tree.nodes.insert(
            id.clone(),
            Node {
                kind: kinds[id].clone(),
                parent: parent.clone(),
            },
        );
    }
    let mut by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for (id, node) in &tree.nodes {
        by_parent
            .entry(node.parent.clone())
            .or_default()
            .push(id.clone());
    }
    // Each parent's children, the same whichever side is ours. A block a side put somewhere (new
    // there, or moved) hangs after the block before it on that side; a block neither moved hangs
    // after the unmoved block before it in the base. Blocks hanging after the same block go
    // moved and new ones first, by id, then the unmoved one, as an insert goes straight after
    // the block it follows.
    let side_tree = |from: From| match from {
        From::Ours => &o,
        From::Theirs => &t,
        From::Base => &b,
    };
    let changed: HashSet<String> = source
        .iter()
        .filter(|(id, from)| {
            let from = **from;
            from != From::Base
                && (b.nodes.get(*id).is_none_or(|n| n.parent != parent_of[*id])
                    || !places.same_order(side_tree(from), id))
        })
        .map(|(id, _)| id.clone())
        .collect();
    for (parent, members) in by_parent {
        let members: HashSet<String> = members.into_iter().collect();
        let base_list = b.siblings(&parent).to_vec();
        let mut anchor: HashMap<String, Option<String>> = HashMap::new();
        for id in &members {
            let a = if changed.contains(id) {
                let list = side_tree(source[id]).siblings(&parent);
                let at = list.iter().position(|x| x == id).unwrap_or(0);
                list[..at]
                    .iter()
                    .rev()
                    .find(|x| members.contains(*x))
                    .cloned()
            } else {
                let at = base_list.iter().position(|x| x == id).unwrap_or(0);
                base_list[..at]
                    .iter()
                    .rev()
                    .find(|x| members.contains(*x) && !changed.contains(*x))
                    .cloned()
            };
            anchor.insert(id.clone(), a);
        }
        let list = loop {
            let mut after: HashMap<Option<String>, Vec<String>> = HashMap::new();
            for (id, a) in &anchor {
                after.entry(a.clone()).or_default().push(id.clone());
            }
            for kids in after.values_mut() {
                kids.sort_by(|x, y| (!changed.contains(x), x).cmp(&(!changed.contains(y), y)));
            }
            let mut list = Vec::with_capacity(members.len());
            let mut stack: Vec<String> = after
                .get(&None)
                .map(|k| k.iter().rev().cloned().collect())
                .unwrap_or_default();
            while let Some(id) = stack.pop() {
                if let Some(kids) = after.get(&Some(id.clone())) {
                    stack.extend(kids.iter().rev().cloned());
                }
                list.push(id);
            }
            if list.len() == members.len() {
                break list;
            }
            // Moves on two sides hung blocks after each other: those go first.
            let placed: HashSet<&String> = list.iter().collect();
            let stuck: Vec<String> = members
                .iter()
                .filter(|id| !placed.contains(id))
                .cloned()
                .collect();
            for id in stuck {
                anchor.insert(id, None);
            }
        };
        tree.children.insert(parent, list);
    }
    tree.children.entry(ROOT.into()).or_default();
    (tree.document(front_matter), conflicts)
}

/// Which edit a block's place comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum From {
    Ours,
    Theirs,
    /// Neither edit has it any more; it's back where the base had it.
    Base,
}

/// A side (by its address) and a parent in it.
type SideParent = (*const Tree, String);

/// Which blocks keep their place among the siblings a side shares with the base, worked out
/// once per side and parent.
struct Places<'a> {
    base: &'a Tree,
    stays: RefCell<HashMap<SideParent, Rc<HashSet<String>>>>,
}

impl<'a> Places<'a> {
    fn new(base: &'a Tree) -> Self {
        Self {
            base,
            stays: RefCell::default(),
        }
    }

    /// Whether `id` keeps its place among the siblings it shares with the base in `side`: the
    /// blocks before and after it there, among blocks both have, are the same.
    fn same_order(&self, side: &Tree, id: &str) -> bool {
        let base = self.base;
        let parent = &base.nodes[id].parent;
        let key = (side as *const Tree, parent.clone());
        if let Some(stay) = self.stays.borrow().get(&key) {
            return stay.contains(id);
        }
        let common = |tree: &Tree| -> Vec<String> {
            tree.siblings(parent)
                .iter()
                .filter(|x| {
                    base.nodes.get(*x).is_some_and(|n| n.parent == *parent)
                        && side.nodes.get(*x).is_some_and(|n| n.parent == *parent)
                })
                .cloned()
                .collect()
        };
        let stay = Rc::new(lcs(&common(base), &common(side)));
        let keeps = stay.contains(id);
        self.stays.borrow_mut().insert(key, stay);
        keeps
    }
}

/// The content of a block both sides have, merged.
fn merge_kind(
    id: &str,
    base: &Kind,
    ours: &Kind,
    theirs: &Kind,
    conflicts: &mut Vec<Conflict>,
) -> Kind {
    if ours == base || ours == theirs {
        return theirs.clone();
    }
    if theirs == base {
        return ours.clone();
    }
    let merged = match (base, ours, theirs) {
        (Kind::Paragraph { text: b }, Kind::Paragraph { text: o }, Kind::Paragraph { text: t }) => {
            merge_text(b, o, t).map(|text| Kind::Paragraph { text })
        }
        (Kind::Cell { text: b }, Kind::Cell { text: o }, Kind::Cell { text: t }) => {
            merge_text(b, o, t).map(|text| Kind::Cell { text })
        }
        (
            Kind::Heading { level: bl, text: b },
            Kind::Heading { level: ol, text: o },
            Kind::Heading { level: tl, text: t },
        ) => merge_field(bl, ol, tl)
            .and_then(|level| merge_text(b, o, t).map(|text| Kind::Heading { level, text })),
        (
            Kind::Code {
                language: bl,
                meta: bm,
                code: b,
            },
            Kind::Code {
                language: ol,
                meta: om,
                code: o,
            },
            Kind::Code {
                language: tl,
                meta: tm,
                code: t,
            },
        ) => merge_field(bl, ol, tl).and_then(|language| {
            merge_field(bm, om, tm).and_then(|meta| {
                merge_text(b, o, t).map(|code| Kind::Code {
                    language,
                    meta,
                    code,
                })
            })
        }),
        _ => None,
    };
    merged.unwrap_or_else(|| {
        conflicts.push(Conflict::Content {
            id: id.into(),
            theirs: theirs.clone(),
        });
        ours.clone()
    })
}

/// A field one side changed, or `None` when both changed it differently.
fn merge_field<T: Clone + PartialEq>(base: &T, ours: &T, theirs: &T) -> Option<T> {
    if ours == base || ours == theirs {
        Some(theirs.clone())
    } else if theirs == base {
        Some(ours.clone())
    } else {
        None
    }
}

/// A three-way merge of text, by character: both sides' edits, or `None` when they overlap.
pub(crate) fn merge_text(base: &str, ours: &str, theirs: &str) -> Option<String> {
    if ours == base || ours == theirs {
        return Some(theirs.into());
    }
    if theirs == base {
        return Some(ours.into());
    }
    let b: Vec<char> = base.chars().collect();
    let o: Vec<char> = ours.chars().collect();
    let t: Vec<char> = theirs.chars().collect();
    let (ho, ht) = (hunks(&b, &o)?, hunks(&b, &t)?);
    // Every pair of edits, one from each side, must keep apart.
    for x in &ho {
        for y in &ht {
            let overlap = x.start < y.end && y.start < x.end;
            let same_point = x.start == y.start
                && (x.start == x.end || y.start == y.end)
                && !(x.start == x.end && y.start == y.end && x.with == y.with);
            let inside = (x.start == x.end && y.start < x.start && x.start < y.end)
                || (y.start == y.end && x.start < y.start && y.start < x.end);
            if (overlap || same_point || inside)
                && !(x.start == y.start && x.end == y.end && x.with == y.with)
            {
                return None;
            }
        }
    }
    let mut all: Vec<&Hunk> = ho.iter().collect();
    for y in &ht {
        if !ho
            .iter()
            .any(|x| x.start == y.start && x.end == y.end && x.with == y.with)
        {
            all.push(y);
        }
    }
    all.sort_by_key(|h| (h.start, h.end));
    let mut out = String::with_capacity(base.len().max(ours.len()).max(theirs.len()));
    let mut at = 0;
    for h in all {
        out.extend(&b[at..h.start]);
        out.extend(&h.with);
        at = h.end;
    }
    out.extend(&b[at..]);
    Some(out)
}

/// An edit to base text: `start..end` (characters) becomes `with`.
#[derive(Debug, PartialEq)]
struct Hunk {
    start: usize,
    end: usize,
    with: Vec<char>,
}

/// The edits that turn `base` into `side`, from a longest common subsequence of characters
/// (after the common start and end), or `None` for text too long to compare that way.
fn hunks(base: &[char], side: &[char]) -> Option<Vec<Hunk>> {
    let pre = base.iter().zip(side).take_while(|(a, b)| a == b).count();
    let max_suf = (base.len() - pre).min(side.len() - pre);
    let suf = base
        .iter()
        .rev()
        .zip(side.iter().rev())
        .take(max_suf)
        .take_while(|(a, b)| a == b)
        .count();
    let (bm, sm) = (&base[pre..base.len() - suf], &side[pre..side.len() - suf]);
    if bm.len() * sm.len() > 4_000_000 {
        return Some(vec![Hunk {
            start: pre,
            end: pre + bm.len(),
            with: sm.to_vec(),
        }]);
    }
    let (n, m) = (bm.len(), sm.len());
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if bm[i] == sm[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut out: Vec<Hunk> = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<Hunk> = None;
    while i < n || j < m {
        if i < n && j < m && bm[i] == sm[j] {
            if let Some(h) = open.take() {
                out.push(h);
            }
            i += 1;
            j += 1;
            continue;
        }
        let h = open.get_or_insert(Hunk {
            start: pre + i,
            end: pre + i,
            with: Vec::new(),
        });
        if j < m && (i == n || table[i][j + 1] >= table[i + 1][j]) {
            h.with.push(sm[j]);
            j += 1;
        } else {
            i += 1;
            h.end = pre + i;
        }
    }
    if let Some(h) = open {
        out.push(h);
    }
    Some(out)
}

#[cfg(test)]
mod tests;
