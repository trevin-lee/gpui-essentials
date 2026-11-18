//! Columns: their Markdown, and that no edit leaves them out of shape.

use super::*;

const TWO: &str = "a\n\n<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\n\
                   right\n\nmore\n\n<!-- /columns -->\n\nb";

/// The kinds of the top-level blocks, and each columns block's columns: their shares of the
/// row (as written) and how many blocks each holds.
fn shape(m: &BlockEditorModel) -> Vec<String> {
    m.document()
        .root()
        .children()
        .map(|b| match b.kind() {
            BlockKind::Columns => {
                let widths: Vec<f32> = b.children().map(|c| c.column_width().unwrap()).collect();
                format!(
                    "columns[{}]",
                    b.children()
                        .zip(crate::written_widths(&widths))
                        .map(|(c, w)| format!("{}:{}", crate::width_text(w), c.children().count()))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
            kind => format!("{kind:?}"),
        })
        .collect()
}

/// Every columns block is at the top level with 2 to 5 columns, every column in one, and the
/// document's Markdown reads back as the same shape.
fn assert_columns_in_shape(m: &BlockEditorModel) {
    assert_invariants(m);
    let source = m.source();
    let back = model(&source);
    assert_eq!(shape(&back), shape(m), "{source}");
}

#[test]
fn columns_round_trip_through_markdown() {
    let m = model(TWO);
    assert_eq!(
        shape(&m),
        ["Paragraph", "columns[0.5:1 0.5:2]", "Paragraph"]
    );
    assert_eq!(m.source(), TWO);
    // A bare column takes an even share, and is written with it.
    let bare = "<!-- columns -->\n\n<!-- column -->\n\nx\n\n<!-- column -->\n\ny\n\n<!-- column -->\n\nz\n\n<!-- /columns -->";
    let m = model(bare);
    assert_eq!(m.source(), bare.replace("column -->", "column 0.333 -->"));
    // Three thirds written to 3 decimals read back without drifting.
    assert_eq!(model(&m.source()).source(), m.source());
    // Shares that don't add up to 1 are made to.
    let m = model(
        "<!-- columns -->\n\n<!-- column 0.5 -->\n\nx\n\n<!-- column 0.25 -->\n\ny\n\n<!-- /columns -->",
    );
    assert!(
        m.source().contains("<!-- column 0.667 -->"),
        "{}",
        m.source()
    );
    assert!(
        m.source().contains("<!-- column 0.333 -->"),
        "{}",
        m.source()
    );
}

#[test]
fn markers_that_dont_make_columns_stay_as_they_were() {
    for source in [
        // A column outside columns.
        "<!-- column 0.5 -->\n\nx",
        // No end.
        "<!-- columns -->\n\n<!-- column -->\n\nx\n\n<!-- column -->\n\ny",
        // One column.
        "<!-- columns -->\n\n<!-- column -->\n\nx\n\n<!-- /columns -->",
        // A bad width.
        "<!-- columns -->\n\n<!-- column 3 -->\n\nx\n\n<!-- column -->\n\ny\n\n<!-- /columns -->",
        // Inside a quote: only the top level has columns.
        "> <!-- columns -->\n>\n> <!-- column -->\n>\n> x\n>\n> <!-- column -->\n>\n> y\n>\n> <!-- /columns -->",
    ] {
        let m = model(source);
        assert!(
            !shape(&m).iter().any(|s| s.starts_with("columns")),
            "{source}: {:?}",
            shape(&m)
        );
        assert_eq!(m.source(), source);
    }
}

#[test]
fn an_empty_column_has_a_line_to_type_in() {
    let m =
        model("<!-- columns -->\n\n<!-- column -->\n\n<!-- column -->\n\ny\n\n<!-- /columns -->");
    assert_eq!(shape(&m), ["columns[0.5:1 0.5:1]"]);
    assert_columns_in_shape(&m);
}

#[test]
fn typing_and_enter_stay_in_their_column() {
    let mut m = model(TWO);
    // "left" is the second leaf.
    caret_at_end(&mut m, 1);
    m.split_block().unwrap();
    m.insert_text("new", EditOrigin::Typing).unwrap();
    assert_eq!(
        shape(&m),
        ["Paragraph", "columns[0.5:2 0.5:2]", "Paragraph"]
    );
    assert_columns_in_shape(&m);
}

#[test]
fn emptying_a_column_takes_it_away_and_one_left_unwraps() {
    let mut m = model(TWO);
    // Backspace at the start of the right column joins its first line to the left column's.
    caret_in(&mut m, 2, 0);
    m.delete(false).unwrap();
    assert_columns_in_shape(&m);
    // Delete everything in the right column, a line at a time.
    let mut m = model(
        "<!-- columns -->\n\n<!-- column -->\n\nx\n\n<!-- column -->\n\ny\n\n<!-- column -->\n\nz\n\n<!-- /columns -->",
    );
    select(&mut m, 2, 0..1);
    m.delete(false).unwrap();
    m.delete(false).unwrap();
    assert_columns_in_shape(&m);
    assert!(
        shape(&m).iter().any(|s| s.starts_with("columns[")),
        "{:?}",
        shape(&m)
    );
}

#[test]
fn a_selection_across_columns_deletes_into_shape() {
    let mut m = model(TWO);
    let first = m.document().leaves().nth(1).unwrap().id();
    let last = m.document().leaves().nth(3).unwrap().id();
    m.set_selection(BlockSelection::Text {
        anchor: BlockPosition::new(first, 2),
        head: BlockPosition::new(last, 2),
    })
    .unwrap();
    m.delete(false).unwrap();
    assert_columns_in_shape(&m);
    // From before the columns to after them.
    let mut m = model(TWO);
    let first = m.document().leaves().next().unwrap().id();
    let last = m.document().leaves().last().unwrap().id();
    m.set_selection(BlockSelection::Text {
        anchor: BlockPosition::new(first, 0),
        head: BlockPosition::new(last, 1),
    })
    .unwrap();
    m.delete(false).unwrap();
    assert_columns_in_shape(&m);
    // All of it.
    let mut m = model(TWO);
    m.run(EditorCommand::SelectAll).unwrap();
    m.run(EditorCommand::DeleteSelection).unwrap();
    assert_columns_in_shape(&m);
}

#[test]
fn turning_wrapping_and_indenting_in_a_column_keep_it_in_shape() {
    for command in [
        EditorCommand::ToggleBlockquote,
        EditorCommand::ToggleList { ordered: false },
        EditorCommand::ToggleTask,
        EditorCommand::WrapInToggle,
        EditorCommand::SetCallout("note".into()),
        EditorCommand::SetBlockKind(BlockKind::Heading { level: 2 }),
        EditorCommand::SetCodeBlock { lang: None },
        EditorCommand::Indent,
        EditorCommand::Outdent,
        EditorCommand::DuplicateBlocks,
    ] {
        let mut m = model(TWO);
        caret_in(&mut m, 2, 1);
        let _ = m.run(command.clone());
        assert_columns_in_shape(&m);
        assert!(
            shape(&m).iter().any(|s| s.starts_with("columns[")),
            "{command:?}: {:?}",
            shape(&m)
        );
    }
}

#[test]
fn columns_pasted_below_the_top_level_become_their_blocks() {
    let pasted =
        "<!-- columns -->\n\n<!-- column -->\n\nx\n\n<!-- column -->\n\ny\n\n<!-- /columns -->";
    // Into a list item: no columns there, but nothing is lost.
    let mut m = model("- item");
    caret_at_end(&mut m, 0);
    m.run(EditorCommand::InsertMarkdown(pasted.into())).unwrap();
    assert_columns_in_shape(&m);
    assert!(!shape(&m).iter().any(|s| s.starts_with("columns[")));
    let text: Vec<_> = m
        .document()
        .leaves()
        .filter_map(|l| l.text().map(str::to_string))
        .collect();
    assert!(
        text.contains(&"x".to_string()) && text.contains(&"y".to_string()),
        "{text:?}"
    );
    // At the top level: columns.
    let mut m = model("");
    m.run(EditorCommand::InsertMarkdown(pasted.into())).unwrap();
    assert_columns_in_shape(&m);
    assert!(
        shape(&m).iter().any(|s| s.starts_with("columns[")),
        "{:?}",
        shape(&m)
    );
}

#[test]
fn moving_blocks_keeps_columns_in_shape() {
    // The columns block down past the last paragraph, and back with undo.
    let mut m = model(TWO);
    let ids: Vec<BlockId> = m.document().root().children().map(|b| b.id()).collect();
    m.set_selection(BlockSelection::Blocks {
        anchor: ids[1],
        head: ids[1],
    })
    .unwrap();
    let _ = m.run(EditorCommand::MoveBlocks { down: true });
    assert_columns_in_shape(&m);
    assert_eq!(shape(&m)[2], "columns[0.5:1 0.5:2]");
    assert!(m.undo());
    assert_columns_in_shape(&m);
    // A line in a column moves within it.
    let mut m = model(TWO);
    caret_in(&mut m, 2, 1);
    let _ = m.run(EditorCommand::MoveBlocks { down: true });
    assert_columns_in_shape(&m);
}

#[test]
fn insert_columns_makes_the_line_the_first_column() {
    let mut m = model("one\n\ntwo");
    caret_in(&mut m, 1, 1);
    m.run(EditorCommand::InsertColumns { count: 3 }).unwrap();
    assert_eq!(shape(&m), ["Paragraph", "columns[0.333:1 0.333:1 0.333:1]"]);
    assert_columns_in_shape(&m);
    // The caret stays in its line, and typing goes there.
    m.insert_text("!", EditOrigin::Typing).unwrap();
    assert!(m.source().contains("t!wo"), "{}", m.source());
    // Not inside columns again, and never more than five.
    assert!(m.run(EditorCommand::InsertColumns { count: 2 }).is_err());
    let mut m = model("x");
    caret_in(&mut m, 0, 0);
    m.run(EditorCommand::InsertColumns { count: 9 }).unwrap();
    assert_eq!(shape(&m), ["columns[0.2:1 0.2:1 0.2:1 0.2:1 0.2:1]"]);
    // In a list item: the whole list goes in the first column.
    let mut m = model("- a\n- b");
    caret_in(&mut m, 1, 1);
    m.run(EditorCommand::InsertColumns { count: 2 }).unwrap();
    assert_eq!(shape(&m), ["columns[0.5:1 0.5:1]"]);
    assert_columns_in_shape(&m);
}

#[test]
fn column_widths_are_set_as_one_step_and_checked() {
    let mut m = model(TWO);
    let columns = m.document().root().children().nth(1).unwrap().id();
    m.run(EditorCommand::SetColumnWidths {
        columns,
        widths: vec![0.3, 0.7],
    })
    .unwrap();
    assert_eq!(shape(&m)[1], "columns[0.3:1 0.7:2]");
    assert!(m.source().contains("<!-- column 0.3 -->"));
    assert!(m.undo());
    assert_eq!(shape(&m)[1], "columns[0.5:1 0.5:2]");
    for widths in [vec![0.5], vec![0.5, 0.], vec![0.5, f32::NAN]] {
        assert!(
            m.run(EditorCommand::SetColumnWidths { columns, widths })
                .is_err()
        );
    }
    let paragraph = m.document().root().children().next().unwrap().id();
    assert!(
        m.run(EditorCommand::SetColumnWidths {
            columns: paragraph,
            widths: vec![0.5, 0.5]
        })
        .is_err()
    );
}
