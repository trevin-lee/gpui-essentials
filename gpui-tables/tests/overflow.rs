//! No text runs out of its cell, at any width. Rows of the worst text a person or an agent
//! could write in every type (a word with nowhere to break, a 300-character address, Japanese,
//! Arabic and emoji, numbers of twenty digits), under headers and options with the same names,
//! are laid out with Core Text (as on screen) in windows 360, 700 and 1400 wide, with the
//! columns at their widths and at their narrowest, grouped and not, with a menu and a picker
//! open, with each typed value's field open in place, with a row's handle, the rows' menu
//! and a row carried, and with a calculation under every column and a footer's menu open;
//! and on a board's cards and a list's lines, with every property on them; and in the bar of
//! views and its menus (and its search field, with "No results" under it), with the views'
//! names at their worst, by a table, a board and a list, with a finger's sizes too. Any glyph or icon drawn past what clips it, past the table
//! or past the window fails the test with where it is. The places a glyph may cross its clip are
//! the edges of the strip the columns after the title scroll in (and the strip the bar's tabs
//! scroll in), where they're cut off until scrolled to, and the edges of a field being typed in, whose text scrolls in it.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::{Select, TableHost, TableView};
use gpui_tables::*;
use gpui_tables::{Filter, Sort, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const WORD: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism-Floccinaucinihilipilification";

fn worst() -> String {
    format!(
        "職務経歴書と志望動機を来週の月曜日までに提出してください。يرجى إرسال خطة الدراسة المحدثة 🎓📚✅🚀 {WORD} https://example.com/{}",
        "x".repeat(260)
    )
}

/// The People database with every name, option and value at its worst, and its rows.
fn hostile() -> (Doc, Vec<Doc>) {
    let (mut db, mut rows) = load("people");
    let fields = db.database.as_mut().unwrap();
    fields.properties.push(Property::new(
        "by",
        "Added by",
        PropertyType::CreatedBy,
        "aF",
    ));
    fields.properties.push(Property::new(
        "editor",
        "Edited by",
        PropertyType::EditedBy,
        "aG",
    ));
    for p in &mut fields.properties {
        p.name = format!("{} {WORD}", p.name);
        if let Some(options) = &mut p.config.options {
            for o in options {
                o.name = format!("{} {}", o.name, worst());
            }
        }
    }
    // Every mark a header can have: sorts, numbered, and a filter. And a calculation under
    // every column, of its type (the numbers' sum near a double's limit).
    for v in &mut fields.views {
        for p in &fields.properties {
            let calc = match p.kind {
                PropertyType::Number => Calculation::Sum,
                PropertyType::Date | PropertyType::CreatedTime | PropertyType::EditedTime => {
                    Calculation::Range
                }
                PropertyType::Checkbox => Calculation::PercentChecked,
                _ => Calculation::CountUnique,
            };
            v.calculations.insert(p.id.clone(), calc);
        }
        v.sort = vec![Sort::ascending(NAME), Sort::descending(WARMTH)];
        v.filter = Some(Filter::condition(
            NAME,
            FilterOp::IsNotEmpty,
            serde_json::Value::Null,
        ));
    }
    let props = fields.properties.clone();
    let mut extra = Vec::new();
    for (i, r) in rows.iter_mut().enumerate() {
        r.title = format!("{} {}", r.title, worst());
        r.author = Some(Person::agent(format!("agent-{WORD}")));
        r.updated_by = Some(Person::agent(format!("{WORD}-{}", worst())));
        let values = r.values.get_or_insert_with(Values::new);
        for p in &props {
            let v = match p.kind {
                PropertyType::Text => Value::text(worst()),
                PropertyType::Email => Value::text(format!("{}@{}.example", "a".repeat(200), WORD)),
                PropertyType::Url => {
                    Value::text(format!("https://example.com/{}", "y".repeat(2000)))
                }
                PropertyType::Phone => Value::text("+".to_owned() + &"9".repeat(63)),
                PropertyType::Number => Value::number(if i % 2 == 0 {
                    -1.234_567_890_123_4e14
                } else {
                    9.99e300
                }),
                PropertyType::MultiSelect => Value::ids(p.options().iter().map(|o| o.id.clone())),
                PropertyType::Person => Value::People(vec![
                    Person::Owner,
                    Person::agent(format!("agent-{WORD}")),
                    Person::agent("career-planning"),
                ]),
                PropertyType::Relation => Value::ids([KESTREL, BLUEFIN, "a-missing-row"]),
                PropertyType::Date => Value::Date(DateValue {
                    start: NOW,
                    end: Some(NOW + 400 * 86_400_000),
                    include_time: true,
                }),
                _ => continue,
            };
            values.insert(p.id.clone(), v);
        }
        if i == 0 {
            let mut empty = r.clone();
            empty.id = "empty-row".into();
            empty.title = String::new();
            empty.values = Some(Values::new());
            extra.push(empty);
        }
    }
    rows.extend(extra);
    (db, rows)
}

fn host() -> Rc<TestHost> {
    let mut host = TestHost::new();
    let h = Rc::get_mut(&mut host).unwrap();
    for c in &mut h.companies {
        c.title = format!("{} {}", c.title, worst());
    }
    for d in h.databases.get_mut() {
        d.title = format!("{} {WORD}", d.title);
    }
    host
}

/// A test app that lays text out as the screen does (GPUI's test platform otherwise measures
/// every string as nothing, so nothing could overflow).
fn app_with_text() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// Every glyph or icon drawn past the left or right edge of what clips it, of the table (or
/// board) or of the window, except where the table's right edge, or the edges of what scrolls
/// sideways, cut it off.
fn overflows(cx: &mut VisualTestContext) -> Vec<String> {
    overflows_with(cx, None)
}

/// The same, with a field open in place within `field`: its text scrolls in it, so a glyph may
/// be cut off by a clip inside it (but never shown past the table or the window).
/// A + menu's field: the top of its picker, over its first entry.
fn picker_field(cx: &mut VisualTestContext, first: &str) -> Bounds<Pixels> {
    let picker = rect(cx, "menu-picker");
    let row = rect(cx, format!("menu-item-{first}"));
    Bounds::from_corners(picker.origin, point(picker.right(), row.top()))
}

fn overflows_with(cx: &mut VisualTestContext, field: Option<Bounds<Pixels>>) -> Vec<String> {
    draw(cx);
    let table = cx
        .debug_bounds("table")
        .or_else(|| cx.debug_bounds("board"))
        .or_else(|| cx.debug_bounds("list"))
        .or_else(|| cx.debug_bounds("no-results"))
        .expect("the table, board or list is drawn");
    // The columns after the title scroll sideways in a strip that cuts them off at its edges
    // (a board's columns scroll in the board itself).
    let strip = cx.debug_bounds("strip").unwrap_or(table);
    // The bar of views' tabs scroll sideways too, cut off at the edges of their strip.
    let tabs = cx.debug_bounds("view-tabs");
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let width = f32::from(window.viewport_size().width) * scale;
        let (left_edge, right_edge) = (
            f32::from(table.left()) * scale,
            f32::from(table.right()) * scale,
        );
        let (strip_left, strip_right) = (
            f32::from(strip.left()) * scale,
            f32::from(strip.right()) * scale,
        );
        let scene = window.rendered_scene();
        // A glyph's tile has a pixel of padding either side.
        let slack = 2.0 * scale;
        // A field's text is clipped to it with a few pixels' room for ink.
        let field = field.map(|f| {
            let ink = 4.0 * scale;
            (
                f32::from(f.left()) * scale - ink,
                f32::from(f.right()) * scale + ink,
                f32::from(f.top()) * scale - ink,
                f32::from(f.bottom()) * scale + ink,
            )
        });
        let mut out = Vec::new();
        let sprites = scene
            .monochrome_sprites
            .iter()
            .map(|s| (s.bounds, s.content_mask.bounds))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|s| (s.bounds, s.content_mask.bounds)),
            )
            .chain(
                scene
                    .polychrome_sprites
                    .iter()
                    .map(|s| (s.bounds, s.content_mask.bounds)),
            );
        for (b, m) in sprites {
            let (left, right) = (
                b.origin.x.as_f32(),
                b.origin.x.as_f32() + b.size.width.as_f32(),
            );
            let (clip_left, clip_right) = (
                m.origin.x.as_f32(),
                m.origin.x.as_f32() + m.size.width.as_f32(),
            );
            let (clip_top, clip_bottom) = (
                m.origin.y.as_f32(),
                m.origin.y.as_f32() + m.size.height.as_f32(),
            );
            let in_field = field.is_some_and(|(l, r, t, b)| {
                clip_left >= l && clip_right <= r && clip_top >= t && clip_bottom <= b
            });
            let at_tabs = |x: f32, edge: fn(&Bounds<Pixels>) -> Pixels| {
                tabs.is_some_and(|t| (x - f32::from(edge(&t)) * scale).abs() <= scale)
            };
            let cut_at_edge = (clip_right - right_edge).abs() <= scale
                || (clip_right - strip_right).abs() <= scale
                || at_tabs(clip_right, Bounds::right)
                || in_field;
            let cut_at_left = (clip_left - strip_left).abs() <= scale
                || at_tabs(clip_left, Bounds::left)
                || in_field;
            let past_clip = (left < clip_left - slack && !cut_at_left)
                || (right > clip_right + slack && !cut_at_edge);
            // What a clip cuts off isn't drawn: past the table or the window is what shows.
            let shown_right = if cut_at_edge {
                right.min(clip_right)
            } else {
                right
            };
            // And on the left, where a board's columns scrolled past its edge are cut off.
            let shown_left = if cut_at_left {
                left.max(clip_left)
            } else {
                left
            };
            let past_table = shown_left < left_edge - slack || shown_right > width + slack;
            if past_clip || past_table {
                out.push(format!(
                    "x {:.0}–{:.0}, y {:.0} (clip {:.0}–{:.0}, table {:.0}–{:.0})",
                    left / scale,
                    right / scale,
                    b.origin.y.as_f32() / scale,
                    clip_left / scale,
                    clip_right / scale,
                    left_edge / scale,
                    right_edge / scale,
                ));
            }
        }
        out
    })
}

fn narrowest(mut db: Doc) -> Doc {
    for v in &mut db.database.as_mut().unwrap().views {
        for c in &mut v.columns {
            c.width = Some(1.);
        }
    }
    // Columns the view lacks get one, at the least width too.
    let f = db.database.as_mut().unwrap();
    let ids: Vec<String> = f.properties.iter().map(|p| p.id.clone()).collect();
    for v in &mut f.views {
        for (i, id) in ids.iter().enumerate() {
            if !v.columns.iter().any(|c| c.property_id == *id) {
                v.columns.push(Column {
                    property_id: id.clone(),
                    width: Some(1.),
                    hidden: false,
                    order: format!("b{i:02}"),
                });
            }
        }
    }
    db
}

#[test]
fn no_cell_header_group_or_menu_lets_its_text_out() {
    let mut found = Vec::new();
    for width in [360., 700., 1400.] {
        for narrow in [false, true] {
            for view in [None, Some(PIPELINE)] {
                let mut app = app_with_text();
                let host = host();
                let (db, rows) = hostile();
                let db = if narrow { narrowest(db) } else { db };
                let db = as_table(db, PIPELINE);
                let (table, cx) =
                    open_with(&mut app, &host, db, rows, view, size(px(width), px(900.)));
                let case = format!(
                    "{width} wide, {}, {}",
                    if narrow { "narrowest" } else { "widths" },
                    view.map_or("table", |_| "grouped")
                );
                for o in overflows(cx) {
                    found.push(format!("{case}: {o}"));
                }
                // A column's menu, a footer's calculations, a picker of options, related rows
                // and people, the month.
                let menus = [
                    ("menu", STAGE),
                    ("calc", WARMTH),
                    ("tags", TAGS),
                    ("company", COMPANY),
                    ("owner", OWNER),
                    ("date", LAST_CONTACTED),
                ];
                for (what, p) in menus {
                    cx.update(|window, cx| {
                        table.update(cx, |t, cx| {
                            t.close_popover(cx);
                            if what == "menu" {
                                t.open_column_menu(p, cx)
                            } else if what == "calc" {
                                let group = t.groups().first().map(|g| g.key.to_string());
                                t.open_calculation_menu(p, group.as_deref(), cx)
                            } else {
                                t.edit(ROWAN, p, window, cx)
                            }
                        })
                    });
                    draw(cx);
                    assert!(
                        table.read_with(cx, |t, _| t.has_popover()),
                        "{case}: {what} opened"
                    );
                    for o in overflows(cx) {
                        found.push(format!("{case}, {what} open: {o}"));
                    }
                }
                // Each typed value's field, open in place with the worst text in it: on one line
                // inside its cell, or for a text value, grown over the rows below inside the
                // window. And a filter's value typed in its column's menu.
                cx.update(|_, cx| table.update(cx, |t, cx| t.close_popover(cx)));
                let typed = [
                    ("title", NAME),
                    ("text", ROLE),
                    ("email", EMAIL),
                    ("url", PROFILE),
                    ("number", WARMTH),
                ];
                for (what, p) in typed {
                    cx.update(|window, cx| table.update(cx, |t, cx| t.edit(ROWAN, p, window, cx)));
                    draw(cx);
                    assert!(
                        table.read_with(cx, |t, cx| t.editing(cx)).is_some(),
                        "{case}: {what}'s field opened"
                    );
                    let field = rect(cx, "editor");
                    let place = rect(cx, cell(ROWAN, p));
                    let window = cx.update(|window, _| window.viewport_size());
                    let inside = if p == ROLE {
                        field.left() >= px(0.)
                            && field.right() <= window.width
                            && field.top() >= place.top() - px(1.)
                    } else {
                        field.left() >= place.left()
                            && field.right() <= place.right()
                            && field.top() >= place.top()
                            && field.bottom() <= place.bottom()
                    };
                    if !inside {
                        found.push(format!(
                            "{case}, {what}'s field at {field:?} leaves its cell {place:?}"
                        ));
                    }
                    for o in overflows_with(cx, Some(field)) {
                        found.push(format!("{case}, {what}'s field open: {o}"));
                    }
                    keys(cx, "escape");
                    assert!(table.read_with(cx, |t, cx| t.editing(cx)).is_none());
                }
                cx.update(|_, cx| table.update(cx, |t, cx| t.open_column_menu(ROLE, cx)));
                draw(cx);
                click(cx, "menu-item-Contains…");
                type_text(cx, &worst());
                let field = rect(cx, "editor");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, a filter's field open: {o}"));
                }
                keys(cx, "escape");
                // The property menus: the + menu with the worst name typed, a select's options
                // and an option's submenu beside them, the types, a number's format, a
                // relation's databases, the delete confirm, and the rename field.
                cx.update(|window, cx| {
                    table.update(cx, |t, cx| {
                        t.close_popover(cx);
                        t.open_add_property_menu(window, cx)
                    })
                });
                draw(cx);
                type_text(cx, &worst());
                let field = picker_field(cx, "Text");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, the + menu open: {o}"));
                }
                keys(cx, "escape");
                let first_option = table.read_with(cx, |t, _| {
                    let f = t.database().database.as_ref().unwrap();
                    f.property(STAGE).unwrap().options()[0].name.clone()
                });
                let pages: [(&str, &[String]); 6] = [
                    (
                        STAGE,
                        &[
                            "menu-item-Edit options".into(),
                            format!("menu-item-{first_option}"),
                        ],
                    ),
                    (STAGE, &["menu-item-Edit options".into()]),
                    (ROLE, &["menu-item-Change type".into()]),
                    (WARMTH, &["menu-item-Number format".into()]),
                    (COMPANY, &["menu-item-Relation".into()]),
                    (ROLE, &["menu-item-Delete property".into()]),
                ];
                for (p, steps) in pages {
                    cx.update(|_, cx| table.update(cx, |t, cx| t.open_column_menu(p, cx)));
                    draw(cx);
                    for s in steps {
                        click(cx, s.clone());
                    }
                    for o in overflows(cx) {
                        found.push(format!("{case}, {steps:?} open: {o}"));
                    }
                    keys(cx, "escape");
                }
                cx.update(|_, cx| table.update(cx, |t, cx| t.open_column_menu(ROLE, cx)));
                draw(cx);
                click(cx, "menu-item-Rename");
                let field = rect(cx, "menu-field");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, the rename field open: {o}"));
                }
                keys(cx, "escape");
                // A row selected (its handle shown) and its menu open, saying the sort places
                // rows; two selected and asking whether to delete them; and a row carried, with
                // the note that the sort places it.
                cx.update(|_, cx| {
                    table.update(cx, |t, cx| {
                        t.close_popover(cx);
                        t.select_row(ROWAN, Select::Only, cx);
                        t.open_row_menu(cx)
                    })
                });
                draw(cx);
                assert!(
                    table.read_with(cx, |t, _| t.has_popover()),
                    "{case}: the rows' menu opened"
                );
                for o in overflows(cx) {
                    found.push(format!("{case}, the rows' menu open: {o}"));
                }
                keys(cx, "escape");
                cx.update(|_, cx| {
                    table.update(cx, |t, cx| t.select_row(IMANI, Select::Toggle, cx))
                });
                keys(cx, "cmd-backspace");
                assert!(drawn(cx, "menu-item-Delete 2 rows"), "{case}: asked");
                for o in overflows(cx) {
                    found.push(format!("{case}, asking to delete two rows: {o}"));
                }
                keys(cx, "escape escape");
                let over = rect(cx, cell(ROWAN, NAME)).center();
                cx.simulate_mouse_move(over, None, Modifiers::none());
                draw(cx);
                let from = rect(cx, format!("handle:{ROWAN}")).center();
                cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
                draw(cx);
                cx.simulate_mouse_move(
                    from + point(px(40.), px(60.)),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                draw(cx);
                assert!(drawn(cx, "drag-note"), "{case}: the drag's note");
                for o in overflows(cx) {
                    found.push(format!("{case}, a row carried: {o}"));
                }
                keys(cx, "escape");
                cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
                draw(cx);
            }
        }
    }
    found.dedup();
    assert!(
        found.is_empty(),
        "{} glyphs past their place:\n{}",
        found.len(),
        found
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The Pipeline board with every property on its cards and every column open.
fn hostile_board() -> (Doc, Vec<Doc>) {
    let (mut db, rows) = hostile();
    let f = db.database.as_mut().unwrap();
    let ids: Vec<String> = f.properties.iter().map(|p| p.id.clone()).collect();
    let v = f.views.iter_mut().find(|v| v.id == PIPELINE).unwrap();
    v.card_properties = ids;
    v.collapsed_groups.clear();
    (db, rows)
}

#[test]
fn no_card_column_or_folded_line_lets_its_text_out() {
    let mut found = Vec::new();
    for width in [360., 700., 1400.] {
        for touch in [false, true] {
            let mut app = app_with_text();
            let host = if touch {
                let h = host();
                let t = h.theme.borrow().clone().with_touch(true);
                *h.theme.borrow_mut() = t;
                h
            } else {
                host()
            };
            let (db, rows) = hostile_board();
            let (table, cx) = open_with(
                &mut app,
                &host,
                db,
                rows,
                Some(PIPELINE),
                size(px(width), px(900.)),
            );
            assert!(table.update(cx, |t, _| t.is_board()));
            let case = format!("board {width} wide{}", if touch { ", touch" } else { "" });
            // From the left, then scrolled to the end, with Active folded among the rest.
            for (step, what) in ["left", "end", "folded"].iter().enumerate() {
                match step {
                    1 => table.update(cx, |t, cx| t.set_board_scroll_x(px(100_000.), cx)),
                    2 => cx.update(|window, cx| {
                        table.update(cx, |t, cx| {
                            t.toggle_column(3, window, cx);
                            t.toggle_column(1, window, cx);
                        })
                    }),
                    _ => {}
                }
                draw(cx);
                for o in overflows(cx) {
                    found.push(format!("{case}, {what}: {o}"));
                }
            }
        }
    }
    found.dedup();
    assert!(
        found.is_empty(),
        "{} glyphs past their place:\n{}",
        found.len(),
        found
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// People's "By stage" list with every property on its lines, grouped by stage or not, and
/// every row with an icon of the worst valid kind (a long ZWJ sequence, a very wide glyph, a
/// 16-byte flag). Longer icons are invalid, and draw nothing.
fn hostile_list(grouped: bool) -> (Doc, Vec<Doc>) {
    const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";
    let (mut db, mut rows) = hostile();
    let f = db.database.as_mut().unwrap();
    let ids: Vec<String> = f.properties.iter().map(|p| p.id.clone()).collect();
    let v = f.views.iter_mut().find(|v| v.id == BY_STAGE).unwrap();
    v.card_properties = ids;
    v.sort.clear();
    if !grouped {
        v.group_by = None;
    }
    for (i, r) in rows.iter_mut().enumerate() {
        let icon = match i % 3 {
            0 => "👩🏽‍💻",
            1 => "\u{FDFD}",
            _ => "\u{1F3F3}\u{FE0F}\u{200D}\u{26A7}\u{FE0F}",
        };
        r.extra.insert("icon".into(), icon.into());
        assert_eq!(r.icon(), Some(icon));
    }
    (db, rows)
}

#[test]
fn no_line_or_group_header_of_a_list_lets_its_text_out() {
    const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";
    let mut found = Vec::new();
    for width in [360., 700., 1400.] {
        for touch in [false, true] {
            for grouped in [true, false] {
                let mut app = app_with_text();
                let host = host();
                if touch {
                    let t = host.theme.borrow().clone().with_touch(true);
                    *host.theme.borrow_mut() = t;
                }
                let (db, rows) = hostile_list(grouped);
                let (table, cx) = open_with(
                    &mut app,
                    &host,
                    db,
                    rows,
                    Some(BY_STAGE),
                    size(px(width), px(900.)),
                );
                assert!(table.update(cx, |t, _| t.is_list()));
                let case = format!(
                    "list {width} wide{}{}",
                    if touch { ", touch" } else { "" },
                    if grouped { ", grouped" } else { "" }
                );
                // As drawn, then with a line focused (its border takes room).
                for focus in [false, true] {
                    if focus {
                        table.update(cx, |t, cx| t.focus_cell("empty-row", NAME, cx));
                        table.update(cx, |t, cx| t.focus_cell(ROWAN, NAME, cx));
                    }
                    draw(cx);
                    for o in overflows(cx) {
                        found.push(format!(
                            "{case}{}: {o}",
                            if focus { ", focused" } else { "" }
                        ));
                    }
                }
            }
        }
    }
    found.dedup();
    assert!(
        found.is_empty(),
        "{} glyphs past their place:\n{}",
        found.len(),
        found
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A text that can't wrap, running out of a box too narrow for it, is found; the same text
/// truncated isn't.
/// The People database at its worst, with its views' names at their worst too.
fn hostile_views() -> (Doc, Vec<Doc>) {
    let (mut db, rows) = hostile();
    for v in &mut db.database.as_mut().unwrap().views {
        v.name = format!("{} {}", v.name, worst());
    }
    (db, rows)
}

#[test]
fn no_view_tab_or_view_menu_lets_its_text_out() {
    let mut found = Vec::new();
    for width in [360., 700., 1400.] {
        for touch in [false, true] {
            // A table, a board and a list.
            for name in ["All people", "Pipeline", "Follow up"] {
                let mut app = app_with_text();
                let host = host();
                if touch {
                    let t = host.theme.borrow().clone().with_touch(true);
                    *host.theme.borrow_mut() = t;
                }
                let (db, rows) = hostile_views();
                let view = view_id(&db, &format!("{name} {}", worst()));
                let (table, cx) = open_with(
                    &mut app,
                    &host,
                    db,
                    rows,
                    Some(&view),
                    size(px(width), px(900.)),
                );
                table.update(cx, |t, cx| t.set_view_bar(true, cx));
                draw(cx);
                let case = format!(
                    "{width} wide{}, {}",
                    if touch { ", touch" } else { "" },
                    table.read_with(cx, |t, _| t.view().unwrap().kind.to_string())
                );
                assert!(drawn(cx, "view-bar"), "{case}: the bar");
                let bar = rect(cx, "view-bar");
                let window = cx.update(|window, _| window.viewport_size());
                if bar.right() > window.width + px(1.) {
                    found.push(format!("{case}: the bar is {bar:?}"));
                }
                for c in ["add-view", "view-settings"] {
                    let b = rect(cx, c);
                    if b.right() > bar.right() + px(1.) || b.left() < bar.left() {
                        found.push(format!("{case}: {c} at {b:?} leaves the bar {bar:?}"));
                    }
                }
                for o in overflows(cx) {
                    found.push(format!("{case}: {o}"));
                }
                // The search open in the bar, with a long query found (the rows narrowed) and
                // with the worst one found nowhere ("No results for" it): the field stays in the
                // bar, the tabs keep room beside it, and the message keeps to the window.
                for (query, nothing) in [(WORD.to_owned(), false), (format!("{}0", worst()), true)]
                {
                    cx.update(|window, cx| table.update(cx, |t, cx| t.open_search(window, cx)));
                    draw(cx);
                    type_text(cx, &query);
                    let what = if nothing {
                        "nothing found"
                    } else {
                        "rows found"
                    };
                    assert_eq!(drawn(cx, "no-results"), nothing, "{case}, {what}");
                    let field = rect(cx, "view-search-field");
                    let tabs = rect(cx, "view-tabs");
                    if field.right() > bar.right() + px(1.) || field.left() < bar.left() {
                        found.push(format!(
                            "{case}, {what}: the field {field:?} leaves the bar"
                        ));
                    }
                    if tabs.size.width < px(60.) {
                        found.push(format!(
                            "{case}, {what}: the field leaves the tabs {tabs:?}"
                        ));
                    }
                    for c in ["add-view", "view-settings", "view-search-clear"] {
                        let b = rect(cx, c);
                        if b.right() > bar.right() + px(1.) || b.left() < bar.left() {
                            found.push(format!("{case}, {what}: {c} at {b:?} leaves the bar"));
                        }
                    }
                    for o in overflows_with(cx, Some(field)) {
                        found.push(format!("{case}, the search open, {what}: {o}"));
                    }
                    keys(cx, "escape");
                    keys(cx, "escape");
                    assert!(!drawn(cx, "view-search-field"), "{case}: the search closed");
                }
                // The + menu with the worst name typed.
                cx.update(|window, cx| table.update(cx, |t, cx| t.open_add_view_menu(window, cx)));
                draw(cx);
                type_text(cx, &worst());
                let field = picker_field(cx, "Table");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, the + menu open: {o}"));
                }
                keys(cx, "escape");
                // The view's menu and its submenus beside it, and the delete confirm.
                let pages: [&[&str]; 6] = [
                    &[],
                    &["menu-item-View settings"],
                    &["menu-item-View settings", "menu-item-Group by"],
                    &["menu-item-Change type"],
                    &["menu-item-Delete view"],
                    &["menu-item-View settings", "menu-item-Card properties"],
                ];
                for steps in pages {
                    cx.update(|window, cx| {
                        table.update(cx, |t, cx| {
                            t.open_view_menu(&view, gpui_tables::view::ViewMenu::Menu, window, cx)
                        })
                    });
                    draw(cx);
                    let mut ok = true;
                    for s in steps {
                        if !drawn(cx, *s) {
                            ok = false;
                            break;
                        }
                        click(cx, *s);
                    }
                    if ok {
                        for o in overflows(cx) {
                            found.push(format!("{case}, {steps:?} open: {o}"));
                        }
                    }
                    keys(cx, "escape");
                }
                cx.update(|window, cx| {
                    table.update(cx, |t, cx| {
                        t.open_view_menu(&view, gpui_tables::view::ViewMenu::Rename, window, cx)
                    })
                });
                draw(cx);
                let field = rect(cx, "menu-field");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, the rename field open: {o}"));
                }
                keys(cx, "escape");
            }
        }
    }
    found.dedup();
    assert!(
        found.is_empty(),
        "{} glyphs past their place:\n{}",
        found.len(),
        found
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The glyphs drawn clipped to `b` (a label or a hint that cuts its text off): those of its
/// text, not what's drawn under it.
fn glyphs_in(cx: &mut VisualTestContext, b: Bounds<Pixels>) -> usize {
    draw(cx);
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let scene = window.rendered_scene();
        // A clip inside `b`, give or take a few pixels for ink.
        let inside = |m: &ContentMask<ScaledPixels>| {
            let (l, r) = (
                m.bounds.origin.x.as_f32() / scale,
                (m.bounds.origin.x.as_f32() + m.bounds.size.width.as_f32()) / scale,
            );
            let (t, bt) = (
                m.bounds.origin.y.as_f32() / scale,
                (m.bounds.origin.y.as_f32() + m.bounds.size.height.as_f32()) / scale,
            );
            l >= f32::from(b.left()) - 4.
                && r <= f32::from(b.right()) + 4.
                && t >= f32::from(b.top()) - 4.
                && bt <= f32::from(b.bottom()) + 4.
                && m.bounds.size.width.as_f32() > 0.
        };
        scene
            .monochrome_sprites
            .iter()
            .map(|s| &s.content_mask)
            .chain(scene.subpixel_sprites.iter().map(|s| &s.content_mask))
            .filter(|m| inside(m))
            .count()
    })
}

#[test]
fn a_long_hint_is_cut_before_its_label() {
    // Pipeline grouped by Stage, then by Stage named at length: Group by's hint is the whole
    // name, cut short (gpui-core's) before the label is, which keeps every glyph it had.
    let mut app = app_with_text();
    let host = TestHost::new();
    let (db, rows) = load("people");
    let (table, cx) = open_with(
        &mut app,
        &host,
        db,
        rows,
        Some(PIPELINE),
        size(px(1400.), px(900.)),
    );
    table.update(cx, |t, cx| t.set_view_bar(true, cx));
    draw(cx);
    click(cx, "view-settings");
    let short = rect(cx, "menu-label-Group by");
    let whole = glyphs_in(cx, short);
    keys(cx, "escape");
    let (mut db, rows) = load("people");
    let f = db.database.as_mut().unwrap();
    f.properties
        .iter_mut()
        .find(|p| p.id == STAGE)
        .unwrap()
        .name = "Stage of the deal in the pipeline as this quarter closes".into();
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    click(cx, "view-settings");
    let (label, hint, menu) = (
        rect(cx, "menu-label-Group by"),
        rect(cx, "menu-hint-Group by"),
        rect(cx, "menu"),
    );
    assert_eq!(glyphs_in(cx, label), whole, "{label:?}");
    assert!(glyphs_in(cx, hint) >= 2, "some of the name, cut: {hint:?}");
    assert!(label.right() <= hint.left() + px(0.5), "{label:?} {hint:?}");
    assert!(hint.right() <= menu.right(), "{hint:?} in {menu:?}");
    assert!(overflows(cx).is_empty());
}

#[test]
fn the_check_finds_text_past_its_box() {
    struct Boxed(bool, Entity<TableView>);
    impl Render for Boxed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let text = format!("Too long for this box: {WORD}");
            let inner = if self.0 {
                div().whitespace_nowrap().child(text)
            } else {
                div().min_w_0().truncate().child(text)
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(div().w(px(200.)).child(div().flex().child(inner)))
                .child(div().w(px(250.)).h(px(300.)).child(self.1.clone()))
        }
    }
    for (nowrap, should_find) in [(true, true), (false, false)] {
        let mut app = app_with_text();
        setup(&mut app);
        let host = TestHost::new();
        let (db, rows) = load("companies");
        let h: Rc<dyn TableHost> = host.clone();
        let (_, cx) = app.add_window_view(move |_, cx| {
            let table = cx.new(|cx| TableView::new(h, db, rows, cx));
            Boxed(nowrap, table)
        });
        cx.simulate_resize(size(px(300.), px(600.)));
        draw(cx);
        let found = overflows(cx);
        assert_eq!(!found.is_empty(), should_find, "{found:?}");
    }
}
