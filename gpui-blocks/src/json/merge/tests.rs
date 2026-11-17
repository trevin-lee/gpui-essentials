use std::collections::{HashMap, HashSet};
use std::prelude::rust_2024::test;

use super::*;

fn p(id: &str, text: &str) -> Block {
    Block {
        id: id.into(),
        kind: Kind::Paragraph { text: text.into() },
        children: Vec::new(),
    }
}

fn quote(id: &str, children: Vec<Block>) -> Block {
    Block {
        id: id.into(),
        kind: Kind::Quote,
        children,
    }
}

fn doc(blocks: Vec<Block>) -> Document {
    Document {
        format: crate::json::FORMAT.into(),
        version: crate::json::VERSION,
        front_matter: String::new(),
        blocks,
    }
}

fn texts(d: &Document) -> Vec<(String, String)> {
    let tree = Tree::of(d);
    tree.order()
        .into_iter()
        .map(|id| {
            let text = match &tree.nodes[&id].kind {
                Kind::Paragraph { text } => text.clone(),
                other => format!("{other:?}"),
            };
            (id, text)
        })
        .collect()
}

#[test]
fn text_edits_apart_both_apply() {
    assert_eq!(
        merge_text("hello world", "hello brave world", "hello world!").as_deref(),
        Some("hello brave world!")
    );
    assert_eq!(
        merge_text("a b c", "A b c", "a b C").as_deref(),
        Some("A b C")
    );
    // The same edit on both sides is one edit.
    assert_eq!(merge_text("abc", "abXc", "abXc").as_deref(), Some("abXc"));
    // Overlapping edits don't merge.
    assert_eq!(merge_text("hello world", "hello there", "hello you"), None);
    // Two different insertions at one place don't either.
    assert_eq!(merge_text("ab", "aXb", "aYb"), None);
}

/// A long document: moves on both sides are told apart from blocks staying in place, however
/// many siblings there are, and the merge stays quick.
#[test]
fn moves_in_a_long_document_merge_quickly_and_cleanly() {
    let n = 3_000;
    let id = |i: usize| format!("b{i}");
    let base: Vec<Block> = (0..n).map(|i| p(&id(i), &format!("{i}"))).collect();
    // `ids` with `from` moved to just after `after`.
    let moved = |ids: &[String], from: usize, after: usize| -> Vec<String> {
        let mut out: Vec<String> = ids.iter().filter(|x| **x != id(from)).cloned().collect();
        let at = out.iter().position(|x| *x == id(after)).unwrap() + 1;
        out.insert(at, id(from));
        out
    };
    let blocks = |ids: &[String]| -> Vec<Block> {
        ids.iter()
            .map(|x| p(x, x.trim_start_matches('b')))
            .collect()
    };
    let base_ids: Vec<String> = (0..n).map(id).collect();
    // Ours moves block 10 after block 20; theirs moves block 2000 after block 2500.
    let ours = moved(&base_ids, 10, 20);
    let theirs = moved(&base_ids, 2000, 2500);
    let started = std::time::Instant::now();
    let (m, c) = merge(&doc(base), &doc(blocks(&ours)), &doc(blocks(&theirs)));
    let took = started.elapsed();
    assert!(c.is_empty(), "two independent moves: {} conflicts", c.len());
    let got: Vec<String> = m.blocks.iter().map(|b| b.id.clone()).collect();
    assert!(got == moved(&ours, 2000, 2500), "both moves, nothing else");
    assert!(took.as_secs() < 5, "took {took:?}");
}

#[test]
fn edits_to_different_blocks_both_apply() {
    let base = doc(vec![p("a", "one"), p("b", "two"), p("c", "three")]);
    let ours = doc(vec![p("a", "ONE"), p("b", "two"), p("c", "three")]);
    let theirs = doc(vec![p("a", "one"), p("b", "two"), p("c", "THREE")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert!(c.is_empty(), "{c:?}");
    assert_eq!(m, doc(vec![p("a", "ONE"), p("b", "two"), p("c", "THREE")]));
}

#[test]
fn a_moved_block_edited_on_the_other_side_keeps_both() {
    let base = doc(vec![p("a", "one"), p("b", "two"), p("c", "three")]);
    let ours = doc(vec![p("b", "two"), p("c", "three"), p("a", "one")]);
    let theirs = doc(vec![p("a", "one, edited"), p("b", "two"), p("c", "three")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert!(c.is_empty(), "{c:?}");
    assert_eq!(
        m,
        doc(vec![p("b", "two"), p("c", "three"), p("a", "one, edited")])
    );
    // Into a quote, too.
    let ours = doc(vec![quote("q", vec![p("a", "one")]), p("b", "two")]);
    let base = doc(vec![quote("q", vec![]), p("a", "one"), p("b", "two")]);
    let theirs = doc(vec![quote("q", vec![]), p("a", "ONE"), p("b", "two")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert!(c.is_empty());
    assert_eq!(m, doc(vec![quote("q", vec![p("a", "ONE")]), p("b", "two")]));
}

#[test]
fn inserts_keep_their_place_among_their_neighbours() {
    let base = doc(vec![p("a", "a"), p("b", "b"), p("c", "c")]);
    let ours = doc(vec![p("a", "a"), p("x", "x"), p("b", "b"), p("c", "c")]);
    let theirs = doc(vec![p("a", "a"), p("b", "b"), p("y", "y"), p("c", "c")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert!(c.is_empty());
    let ids: Vec<String> = texts(&m).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["a", "x", "b", "y", "c"]);
}

#[test]
fn a_deleted_block_edited_on_the_other_side_is_kept_and_reported() {
    let base = doc(vec![p("a", "one"), p("b", "two")]);
    let ours = doc(vec![p("b", "two")]);
    let theirs = doc(vec![p("a", "one, edited"), p("b", "two")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert_eq!(m, theirs);
    assert_eq!(
        c,
        [Conflict::DeletedChanged {
            id: "a".into(),
            deleted_by: Side::Ours
        }]
    );
    // Unedited, the deletion stands.
    let theirs = base.clone();
    let (m, c) = merge(&base, &ours, &theirs);
    assert_eq!((m, c), (ours, vec![]));
}

#[test]
fn a_deleted_block_with_something_new_inside_is_kept() {
    let base = doc(vec![quote("q", vec![p("a", "one")]), p("b", "two")]);
    let ours = doc(vec![p("b", "two")]);
    let theirs = doc(vec![
        quote("q", vec![p("a", "one"), p("n", "new")]),
        p("b", "two"),
    ]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert_eq!(m, doc(vec![quote("q", vec![p("n", "new")]), p("b", "two")]));
    assert!(c.contains(&Conflict::Restored {
        id: "q".into(),
        deleted_by: Side::Ours
    }));
}

#[test]
fn overlapping_text_edits_keep_ours_and_report_theirs() {
    let base = doc(vec![p("a", "hello world")]);
    let ours = doc(vec![p("a", "hello there")]);
    let theirs = doc(vec![p("a", "hello you")]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert_eq!(m, ours);
    assert_eq!(
        c,
        [Conflict::Content {
            id: "a".into(),
            theirs: Kind::Paragraph {
                text: "hello you".into()
            }
        }]
    );
}

#[test]
fn moves_into_each_other_dont_make_a_loop() {
    let base = doc(vec![quote("x", vec![]), quote("y", vec![])]);
    let ours = doc(vec![quote("x", vec![quote("y", vec![])])]);
    let theirs = doc(vec![quote("y", vec![quote("x", vec![])])]);
    let (m, c) = merge(&base, &ours, &theirs);
    assert_eq!(m, ours);
    assert!(matches!(c.as_slice(), [Conflict::Moved { .. }]));
}

#[test]
fn operations_serialize_flat() {
    let op = Op::Insert {
        id: "n".into(),
        kind: Kind::Paragraph { text: "hi".into() },
        parent: None,
        after: Some("a".into()),
    };
    assert_eq!(
        serde_json::to_string(&op).unwrap(),
        r#"{"op":"insert","id":"n","type":"paragraph","text":"hi","parent":null,"after":"a"}"#
    );
}

#[test]
fn bad_operations_are_refused() {
    let d = doc(vec![quote("q", vec![p("a", "one")])]);
    let bad = [
        Op::Delete { id: "zz".into() },
        Op::Move {
            id: "q".into(),
            parent: Some("a".into()),
            after: None,
        },
        Op::Insert {
            id: "a".into(),
            kind: Kind::Divider,
            parent: None,
            after: None,
        },
        Op::Insert {
            id: "n".into(),
            kind: Kind::Divider,
            parent: None,
            after: Some("a".into()),
        },
    ];
    for op in bad {
        assert!(apply(&d, std::slice::from_ref(&op)).is_err(), "{op:?}");
    }
}

// Properties, over random documents and random edits.

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

const WORDS: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "eps", "zeta", "eta", "theta",
];

fn sentence(r: &mut Rng) -> String {
    (0..1 + r.below(5))
        .map(|_| WORDS[r.below(WORDS.len())])
        .collect::<Vec<_>>()
        .join(" ")
}

struct Ids(usize);

impl Ids {
    fn next(&mut self, prefix: &str) -> String {
        self.0 += 1;
        format!("{prefix}{}", self.0)
    }
}

fn random_doc(r: &mut Rng, ids: &mut Ids) -> Document {
    let mut blocks = Vec::new();
    for _ in 0..2 + r.below(6) {
        if r.chance(25) {
            let kids = (0..r.below(3))
                .map(|_| p(&ids.next("b"), &sentence(r)))
                .collect();
            blocks.push(quote(&ids.next("b"), kids));
        } else {
            blocks.push(p(&ids.next("b"), &sentence(r)));
        }
    }
    doc(blocks)
}

/// One random edit of `d`.
fn edit(d: &Document, r: &mut Rng, ids: &mut Ids, side: &str) -> Document {
    let mut tree = Tree::of(d);
    let order = tree.order();
    let containers: Vec<String> = std::iter::once(ROOT.to_string())
        .chain(
            order
                .iter()
                .filter(|id| tree.nodes[*id].kind == Kind::Quote)
                .cloned(),
        )
        .collect();
    match r.below(5) {
        // Change a paragraph's text.
        0 if !order.is_empty() => {
            let id = &order[r.below(order.len())];
            if let Kind::Paragraph { text } = &mut tree.nodes.get_mut(id).unwrap().kind {
                let mut words: Vec<String> = text.split(' ').map(String::from).collect();
                let at = r.below(words.len());
                words[at] = format!("{}{side}", WORDS[r.below(WORDS.len())]);
                *text = words.join(" ");
            }
        }
        // Insert a paragraph.
        1 => {
            let parent = containers[r.below(containers.len())].clone();
            let list = tree.siblings(&parent).to_vec();
            let after = if list.is_empty() || r.chance(30) {
                None
            } else {
                Some(list[r.below(list.len())].clone())
            };
            let id = ids.next(side);
            tree.nodes.insert(
                id.clone(),
                Node {
                    kind: Kind::Paragraph { text: sentence(r) },
                    parent: parent.clone(),
                },
            );
            tree.children.insert(id.clone(), Vec::new());
            tree.place(&id, &parent, after.as_deref()).unwrap();
        }
        // Delete a block.
        2 if !order.is_empty() => {
            let id = order[r.below(order.len())].clone();
            tree.remove(&id);
        }
        // Move a block.
        3 if !order.is_empty() => {
            let id = order[r.below(order.len())].clone();
            let parent = containers[r.below(containers.len())].clone();
            if parent != id && !(parent != ROOT && tree.contains_in(&id, &parent)) {
                tree.detach(&id);
                let list = tree.siblings(&parent).to_vec();
                let after = if list.is_empty() || r.chance(30) {
                    None
                } else {
                    Some(list[r.below(list.len())].clone())
                };
                tree.place(&id, &parent, after.as_deref()).unwrap();
            }
        }
        _ => {}
    }
    tree.document(d.front_matter.clone())
}

fn edited(d: &Document, r: &mut Rng, ids: &mut Ids, side: &str) -> Document {
    let mut d = d.clone();
    for _ in 0..1 + r.below(4) {
        d = edit(&d, r, ids, side);
    }
    d
}

fn kinds(d: &Document) -> HashMap<String, Kind> {
    Tree::of(d)
        .nodes
        .into_iter()
        .map(|(id, n)| (id, n.kind))
        .collect()
}

#[test]
fn property_diff_then_apply_gives_the_other_document() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let mut ids = Ids(0);
    for _ in 0..2000 {
        let a = random_doc(&mut r, &mut ids);
        let b = if r.chance(20) {
            random_doc(&mut r, &mut ids)
        } else {
            edited(&a, &mut r, &mut ids, "x")
        };
        let ops = diff(&a, &b);
        assert_eq!(apply(&a, &ops).as_ref(), Ok(&b), "{a:?}\n{b:?}\n{ops:?}");
        assert!(diff(&a, &a).is_empty());
    }
}

#[test]
fn property_an_unchanged_side_gives_the_other() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    let mut ids = Ids(0);
    for _ in 0..2000 {
        let base = random_doc(&mut r, &mut ids);
        let side = edited(&base, &mut r, &mut ids, "o");
        assert_eq!(merge(&base, &base, &side), (side.clone(), vec![]));
        assert_eq!(merge(&base, &side, &base), (side.clone(), vec![]));
        assert_eq!(merge(&base, &side, &side).0, side);
    }
}

#[test]
fn property_order_doesnt_matter_without_conflicts_and_nothing_is_lost() {
    let mut r = Rng(0xd1b5_4a32_d192_ed03);
    let mut ids = Ids(0);
    let mut clean = 0;
    for _ in 0..3000 {
        let base = random_doc(&mut r, &mut ids);
        let ours = edited(&base, &mut r, &mut ids, "o");
        let theirs = edited(&base, &mut r, &mut ids, "t");
        let (m1, c1) = merge(&base, &ours, &theirs);
        let (m2, c2) = merge(&base, &theirs, &ours);
        if c1.is_empty() && c2.is_empty() {
            clean += 1;
            assert_eq!(m1, m2, "\nbase {base:?}\nours {ours:?}\ntheirs {theirs:?}");
        }
        // Nothing lost silently.
        let (kb, ko, kt, km) = (kinds(&base), kinds(&ours), kinds(&theirs), kinds(&m1));
        let reported: HashSet<String> = c1
            .iter()
            .filter_map(|c| match c {
                Conflict::Content { id, .. }
                | Conflict::DeletedChanged { id, .. }
                | Conflict::Restored { id, .. }
                | Conflict::Moved { id, .. } => Some(id.clone()),
                Conflict::FrontMatter { .. } => None,
            })
            .collect();
        for (id, k) in ko.iter().chain(kt.iter()) {
            let new = !kb.contains_key(id);
            let changed = kb.get(id).is_some_and(|b| b != k);
            if new || changed {
                assert!(
                    km.contains_key(id) || reported.contains(id),
                    "{id} was lost\nbase {base:?}\nours {ours:?}\ntheirs {theirs:?}\nmerged {m1:?}"
                );
            }
        }
        for (id, k) in &km {
            let from_o = ko.get(id) == Some(k);
            let from_t = kt.get(id) == Some(k);
            let changed_both = ko.get(id) != kb.get(id) && kt.get(id) != kb.get(id);
            assert!(
                from_o || from_t || changed_both || reported.contains(id),
                "{id}'s content came from nowhere"
            );
            // A block one side changed and the other didn't has that side's content.
            if let (Some(b), Some(o), Some(t)) = (kb.get(id), ko.get(id), kt.get(id)) {
                if o == b && t != b {
                    assert_eq!(k, t);
                }
                if t == b && o != b {
                    assert_eq!(k, o);
                }
            }
        }
        // Deleted on both sides, gone.
        for id in kb.keys() {
            if !ko.contains_key(id) && !kt.contains_key(id) {
                assert!(!km.contains_key(id) || reported.contains(id));
            }
        }
        // Every id once, and the result applies as operations too.
        assert_eq!(apply(&base, &diff(&base, &m1)).as_ref(), Ok(&m1));
    }
    assert!(clean > 1000, "too few clean merges to compare: {clean}");
}
