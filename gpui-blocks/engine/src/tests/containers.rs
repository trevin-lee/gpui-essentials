//! Callouts (`> [!NOTE]`) and toggles (`<details><summary>`).

use super::*;
use crate::surfaces::with_default_surfaces;

fn kinds(m: &BlockEditorModel) -> Vec<BlockKind> {
    m.document()
        .root()
        .children()
        .map(|b| b.kind())
        .collect()
}

fn leaf_count(m: &BlockEditorModel) -> usize {
    m.document().leaves().count()
}

#[test]
fn callout_round_trips() {
    for source in [
        "> [!NOTE]\n> Hello",
        "> [!WARNING]\n> one\n>\n> two",
        "> [!TIP]\n> - a\n> - b",
        "before\n\n> [!CAUTION]\n> careful\n\nafter",
    ] {
        let m = model(source);
        assert_eq!(m.source(), source);
        assert!(kinds(&m).contains(&BlockKind::Callout), "{source}");
        assert_invariants(&m);
    }
}

#[test]
fn callout_from_a_registered_surface_round_trips() {
    // With the containers registered, the parser hands top-level callouts
    // over as surfaces; they import the same.
    let source = "> [!NOTE]\n> Hello\n>\n> more";
    let m = BlockEditorModel::from_markdown(source, with_default_surfaces(Default::default()))
        .unwrap();
    assert_eq!(kinds(&m), vec![BlockKind::Callout]);
    assert_eq!(m.source(), source);
}

#[test]
fn a_quote_starting_with_an_escaped_marker_stays_a_quote() {
    // The soft line break is written as a space, as every paragraph's is.
    let m = model("> \\[!NOTE]\n> Hello");
    assert_eq!(kinds(&m), vec![BlockKind::Blockquote]);
    assert_eq!(m.source(), "> \\[!NOTE\\] Hello");
    assert_eq!(model(&m.source()).source(), m.source());
}

#[test]
fn callout_name_survives_and_a_marker_alone_has_an_empty_body() {
    let m = model("> [!note]");
    assert_eq!(kinds(&m), vec![BlockKind::Callout]);
    assert_eq!(leaf_count(&m), 1);
    assert!(m.source().starts_with("> [!NOTE]"));
}

#[test]
fn enter_on_an_empty_last_line_leaves_the_callout() {
    let mut m = model("> [!NOTE]\n> a");
    caret_at_end(&mut m, 0);
    m.split_block().unwrap();
    m.split_block().unwrap();
    m.insert_text("b", EditOrigin::Typing).unwrap();
    assert_eq!(m.source(), "> [!NOTE]\n> a\n\nb");
    assert_invariants(&m);
}

#[test]
fn backspace_at_the_start_of_a_callout_lifts_its_first_line() {
    let mut m = model("> [!NOTE]\n> a");
    caret_in(&mut m, 0, 0);
    m.delete(false).unwrap();
    assert_eq!(m.source(), "a");
    assert!(m.undo());
    assert_eq!(m.source(), "> [!NOTE]\n> a");
}

#[test]
fn set_callout_wraps_then_renames() {
    let mut m = model("a");
    caret_in(&mut m, 0, 1);
    m.run(EditorCommand::SetCallout("Tip".into())).unwrap();
    assert_eq!(m.source(), "> [!TIP]\n> a");
    m.run(EditorCommand::SetCallout("warning".into())).unwrap();
    assert_eq!(m.source(), "> [!WARNING]\n> a");
    m.run(EditorCommand::Unwrap).unwrap();
    assert_eq!(m.source(), "a");
    assert_eq!(m.caret().unwrap().offset, 1);
}

#[test]
fn toggle_round_trips() {
    for source in [
        "<details>\n<summary>Title</summary>\n\nBody\n\n</details>",
        "<details open>\n<summary>Title</summary>\n\nBody\n\nMore\n\n</details>",
        "<details open>\n<summary>**Bold** title</summary>\n\n- a\n- b\n\n</details>",
        "<details>\n<summary>Empty</summary>\n\n</details>",
        "<details open>\n<summary>Outer</summary>\n\n<details>\n<summary>Inner</summary>\n\nx\n\n</details>\n\n</details>",
        "a\n\n<details>\n<summary>T</summary>\n\nb\n\n</details>\n\nc",
    ] {
        let m = model(source);
        assert_eq!(m.source(), source);
        assert!(
            kinds(&m)
                .iter()
                .any(|k| matches!(k, BlockKind::Toggle { .. })),
            "{source}"
        );
        assert_invariants(&m);
    }
}

#[test]
fn details_on_one_line_with_nothing_inside_is_an_empty_toggle() {
    let m = model("<details><summary>T</summary></details>");
    assert_eq!(kinds(&m), vec![BlockKind::Toggle { open: false }]);
    assert_eq!(
        m.source(),
        "<details>\n<summary>T</summary>\n\n</details>"
    );
}

#[test]
fn an_unclosed_details_stays_raw_html() {
    let source = "<details>\n<summary>T</summary>\n\nBody";
    let m = model(source);
    assert!(
        !kinds(&m)
            .iter()
            .any(|k| matches!(k, BlockKind::Toggle { .. }))
    );
    assert_eq!(m.source(), source);
}

#[test]
fn a_closed_toggle_hides_its_body_from_the_caret() {
    let m = model("<details>\n<summary>T</summary>\n\nhidden\n\n</details>\n\nafter");
    let texts: Vec<_> = m
        .document()
        .leaves()
        .map(|b| b.text().unwrap_or_default().to_string())
        .collect();
    assert_eq!(texts, ["T", "after"]);
}

#[test]
fn opening_and_closing_is_an_undoable_step() {
    let mut m = model("<details open>\n<summary>T</summary>\n\nbody\n\n</details>");
    let toggle = m.document().root().children().next().unwrap().id();
    // The caret in the body moves to the summary's end when it closes.
    caret_at_end(&mut m, 1);
    m.run(EditorCommand::SetToggleOpen {
        block: toggle,
        open: false,
    })
    .unwrap();
    assert_eq!(
        m.source(),
        "<details>\n<summary>T</summary>\n\nbody\n\n</details>"
    );
    assert_eq!(leaf_count(&m), 1);
    let caret = m.caret().unwrap();
    assert_eq!(m.document().block(caret.block).unwrap().text(), Some("T"));
    assert_eq!(caret.offset, 1);
    assert!(m.undo());
    assert_eq!(leaf_count(&m), 2);
    assert_invariants(&m);
}

#[test]
fn enter_in_an_open_summary_starts_the_body() {
    let mut m = model("<details open>\n<summary>T</summary>\n\nb\n\n</details>");
    caret_at_end(&mut m, 0);
    m.split_block().unwrap();
    m.insert_text("x", EditOrigin::Typing).unwrap();
    assert_eq!(
        m.source(),
        "<details open>\n<summary>T</summary>\n\nx\n\nb\n\n</details>"
    );
}

#[test]
fn enter_in_a_closed_summary_starts_a_new_toggle() {
    let mut m = model("<details>\n<summary>Tab</summary>\n\nb\n\n</details>");
    caret_in(&mut m, 0, 1);
    m.split_block().unwrap();
    assert_eq!(
        m.source(),
        "<details>\n<summary>T</summary>\n\nb\n\n</details>\n\n<details>\n<summary>ab</summary>\n\n</details>"
    );
    assert_eq!(m.caret().unwrap().offset, 0);
    assert_invariants(&m);
}

#[test]
fn enter_in_an_empty_summary_unwraps_the_toggle() {
    let mut m = model("a");
    caret_in(&mut m, 0, 0);
    m.delete(true).unwrap();
    m.run(EditorCommand::WrapInToggle).unwrap();
    assert_eq!(kinds(&m), vec![BlockKind::Toggle { open: true }]);
    m.split_block().unwrap();
    assert_eq!(kinds(&m), vec![BlockKind::Paragraph]);
}

#[test]
fn backspace_at_the_start_of_a_summary_unwraps_the_toggle() {
    let mut m = model("<details open>\n<summary>T</summary>\n\nB\n\n</details>");
    caret_in(&mut m, 0, 0);
    m.delete(false).unwrap();
    assert_eq!(m.source(), "T\n\nB");
    assert!(m.undo());
    assert_eq!(
        m.source(),
        "<details open>\n<summary>T</summary>\n\nB\n\n</details>"
    );
}

#[test]
fn enter_on_an_empty_last_body_line_leaves_the_toggle() {
    let mut m = model("<details open>\n<summary>T</summary>\n\nb\n\n</details>");
    caret_at_end(&mut m, 1);
    m.split_block().unwrap();
    m.split_block().unwrap();
    m.insert_text("c", EditOrigin::Typing).unwrap();
    assert_eq!(
        m.source(),
        "<details open>\n<summary>T</summary>\n\nb\n\n</details>\n\nc"
    );
}

#[test]
fn wrap_in_toggle_makes_the_line_its_summary() {
    let mut m = model("Title");
    caret_in(&mut m, 0, 5);
    m.run(EditorCommand::WrapInToggle).unwrap();
    m.split_block().unwrap();
    m.insert_text("inside", EditOrigin::Typing).unwrap();
    assert_eq!(
        m.source(),
        "<details open>\n<summary>Title</summary>\n\ninside\n\n</details>"
    );
}

#[test]
fn deleting_across_a_closed_toggle_takes_its_body_too() {
    let mut m = model("a\n\n<details>\n<summary>T</summary>\n\nhidden\n\n</details>\n\nz");
    let first = m.document().leaves().next().unwrap().id();
    let last = m.document().leaves().last().unwrap().id();
    m.set_selection(BlockSelection::Text {
        anchor: BlockPosition::new(first, 1),
        head: BlockPosition::new(last, 0),
    })
    .unwrap();
    m.delete(false).unwrap();
    assert_eq!(m.source(), "az");
    assert_invariants(&m);
    assert!(m.undo());
    assert_eq!(
        m.source(),
        "a\n\n<details>\n<summary>T</summary>\n\nhidden\n\n</details>\n\nz"
    );
}

#[test]
fn a_toggle_in_a_callout_round_trips() {
    let source = "> [!NOTE]\n> <details open>\n> <summary>T</summary>\n>\n> x\n>\n> </details>";
    let m = model(source);
    assert_eq!(m.source(), source);
    assert_eq!(kinds(&m), vec![BlockKind::Callout]);
}

#[test]
fn insert_markdown_replaces_an_empty_line_and_leaves_a_line_after() {
    let mut m = model("a\n\n");
    let last = m.document().leaves().count() - 1;
    caret_in(&mut m, last, 0);
    m.run(EditorCommand::InsertMarkdown("---".into())).unwrap();
    m.insert_text("b", EditOrigin::Typing).unwrap();
    assert_eq!(m.source(), "a\n\n---\n\nb");
    assert!(m.undo());
    assert!(m.undo());
    assert_eq!(m.source(), "a\n\n");
}

#[test]
fn copying_across_a_closed_toggle_keeps_its_body() {
    let source = "a\n\n<details>\n<summary>T</summary>\n\nhidden\n\n</details>\n\nz";
    let mut m = model(source);
    let first = m.document().leaves().next().unwrap().id();
    let last = m.document().leaves().last().unwrap().id();
    m.set_selection(BlockSelection::Text {
        anchor: BlockPosition::new(first, 0),
        head: BlockPosition::new(last, 1),
    })
    .unwrap();
    let range = m.doc().resolve_selection(m.selection()).unwrap();
    let copied = crate::slice::selection_markdown(m.doc(), range).unwrap();
    assert_eq!(copied, source);
    // A cut is a copy and a delete: pasting it back loses nothing.
    m.delete(false).unwrap();
    m.paste_blocks(&copied).unwrap();
    assert_eq!(m.source(), source);
}

#[test]
fn copying_a_closed_toggle_at_the_document_start_keeps_its_body() {
    let source = "<details>\n<summary>T</summary>\n\nhidden\n\n</details>\n\nz";
    let mut m = model(source);
    let first = m.document().leaves().next().unwrap().id();
    let last = m.document().leaves().last().unwrap().id();
    m.set_selection(BlockSelection::Text {
        anchor: BlockPosition::new(first, 0),
        head: BlockPosition::new(last, 1),
    })
    .unwrap();
    let range = m.doc().resolve_selection(m.selection()).unwrap();
    let copied = crate::slice::selection_markdown(m.doc(), range).unwrap();
    assert_eq!(copied, source);
}
