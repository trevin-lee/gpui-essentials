//! No text runs out of its cell, at any width. Rows of the worst text a person or an agent
//! could write in every type (a word with nowhere to break, a 300-character address, Japanese,
//! Arabic and emoji, numbers of twenty digits), under headers and options with the same names,
//! are laid out with Core Text (as on screen) in windows 360, 700 and 1400 wide, with the
//! columns at their widths and at their narrowest, grouped and not, with a menu and a picker
//! open, and with each typed value's field open in place; and on a board's cards and a list's
//! lines, with every property on them. Any glyph or icon drawn past what clips it, past the table
//! or past the window fails the test with where it is. The places a glyph may cross its clip are
//! the edges of the strip the columns after the title scroll in, where they're cut off until
//! scrolled to, and the edges of a field being typed in, whose text scrolls in it.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::{TableHost, TableView};
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
    // Every mark a header can have: sorts, numbered, and a filter.
    for v in &mut fields.views {
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
fn overflows_with(cx: &mut VisualTestContext, field: Option<Bounds<Pixels>>) -> Vec<String> {
    draw(cx);
    let table = cx
        .debug_bounds("table")
        .or_else(|| cx.debug_bounds("board"))
        .or_else(|| cx.debug_bounds("list"))
        .expect("the table, board or list is drawn");
    // The columns after the title scroll sideways in a strip that cuts them off at its edges
    // (a board's columns scroll in the board itself).
    let strip = cx.debug_bounds("strip").unwrap_or(table);
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
            let in_field = field.is_some_and(|(l, r)| clip_left >= l && clip_right <= r);
            let cut_at_edge = (clip_right - right_edge).abs() <= scale
                || (clip_right - strip_right).abs() <= scale
                || in_field;
            let cut_at_left = (clip_left - strip_left).abs() <= scale || in_field;
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
                // A column's menu, a picker of options, related rows and people, the day grid.
                let menus = [
                    ("menu", STAGE),
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
                click(cx, "entry:Contains…");
                type_text(cx, &worst());
                let field = rect(cx, "editor");
                for o in overflows_with(cx, Some(field)) {
                    found.push(format!("{case}, a filter's field open: {o}"));
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
