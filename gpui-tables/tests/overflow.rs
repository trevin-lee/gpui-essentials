//! No text runs out of its cell, at any width. Rows of the worst text a person or an agent
//! could write in every type (a word with nowhere to break, a 300-character address, Japanese,
//! Arabic and emoji, numbers of twenty digits), under headers and options with the same names,
//! are laid out with Core Text (as on screen) in windows 360, 700 and 1400 wide, with the
//! columns at their widths and at their narrowest, grouped and not, with a menu and a picker
//! open. Any glyph or icon drawn past what clips it, past the table or past the window fails
//! the test with where it is. The one place a glyph may cross its clip is the edges of the
//! strip the columns after the title scroll in, where they're cut off until scrolled to.

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

/// Every glyph or icon drawn past the left or right edge of what clips it, of the table or of
/// the window, except across the table's right edge.
fn overflows(cx: &mut VisualTestContext) -> Vec<String> {
    draw(cx);
    let table = cx.debug_bounds("table").expect("the table is drawn");
    // The columns after the title scroll sideways in a strip that cuts them off at its edges.
    let strip = cx.debug_bounds("strip").expect("the header is drawn");
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
            let cut_at_edge = (clip_right - right_edge).abs() <= scale
                || (clip_right - strip_right).abs() <= scale;
            let cut_at_left = (clip_left - strip_left).abs() <= scale;
            let past_clip = (left < clip_left - slack && !cut_at_left)
                || (right > clip_right + slack && !cut_at_edge);
            // What a clip cuts off isn't drawn: past the table or the window is what shows.
            let shown_right = if cut_at_edge {
                right.min(clip_right)
            } else {
                right
            };
            let past_table = left < left_edge - slack || shown_right > width + slack;
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
