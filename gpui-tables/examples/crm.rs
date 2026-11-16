//! The CRM in the fixtures (People, Companies and Interactions, all made up) in a window, with
//! its table and board views and a log of every event they emit.
//!
//! ```sh
//! cargo run --release -p gpui-tables --example crm
//! cargo run --release -p gpui-tables --features shots --example crm -- --shot out.png [--scene menu]
//! ```
//!
//! The host here keeps the docs in memory and applies the edits itself (both sides of a two-way
//! relation too), so the table changes as you use it. Typed values (titles, text, numbers,
//! emails...) go through `TableHost::edit_text`, which this example answers with a bare
//! one-line field of its own: gpui-core has no text input yet.
//!
//! `--shot` takes a picture of the window without showing it. Scenes: `people` (the default),
//! `grouped` (Interactions by month), `menu` (a column's menu), `picker` (a select's options),
//! `date` (the day grid), `scrolled` (scrolled sideways, the title column in place), `light`
//! and `worst` (worst-case text, every cell ending in an ellipsis); and boards: `board` (People's
//! Pipeline by stage), `board-drag` (a card of Interactions by kind dragged to another column),
//! `board-light` (Companies by industry, in the light theme) and `board-worst`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Press, Selectable as _, Theme};
use gpui_tables::engine::relation;
use gpui_tables::view::{RelatedRow, TableHost, TableTheme, TableView, TextEdit};
use gpui_tables::*;

const PEOPLE: &str = include_str!("../tests/fixtures/people.json");
const COMPANIES: &str = include_str!("../tests/fixtures/companies.json");
const INTERACTIONS: &str = include_str!("../tests/fixtures/interactions.json");

/// The fixtures' "now": 2026-11-15, noon UTC.
const NOW: i64 = 1_794_744_000_000;

struct Args {
    shot: Option<PathBuf>,
    scene: String,
}

fn args() -> Args {
    let mut out = Args {
        shot: None,
        scene: "people".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shot" => out.shot = it.next().map(PathBuf::from),
            "--scene" => out.scene = it.next().unwrap_or_default(),
            _ => {}
        }
    }
    out
}

/// A database and its rows.
struct Db {
    doc: Doc,
    rows: Vec<Doc>,
}

fn load(json: &str) -> Db {
    let docs: Vec<Doc> = serde_json::from_str(json).expect("a fixture");
    let (dbs, rows): (Vec<Doc>, Vec<Doc>) =
        docs.into_iter().partition(|d| d.kind == DocKind::Database);
    Db {
        doc: dbs.into_iter().next().expect("a database"),
        rows,
    }
}

/// A row of the worst text a person or an agent could write, for the `worst` scene.
fn worst_row(db: &Db) -> Doc {
    let word = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism";
    let fields = db.doc.database.as_ref().unwrap();
    let mut values = Values::new();
    for p in &fields.properties {
        let v = match p.kind {
            PropertyType::Text => Value::text(format!(
                "職務経歴書と志望動機 يرجى إرسال 🎓📚 {word} {word}"
            )),
            PropertyType::Email => Value::text(format!(
                "{}@{}.example",
                "x".repeat(60),
                word.to_lowercase()
            )),
            PropertyType::Phone => Value::text("+1 (555) 0100-0199-0123-4567-8901-2345"),
            PropertyType::Url => Value::text(format!("https://example.com/{}", "y".repeat(200))),
            PropertyType::Number => Value::number(123_456_789_012_345.67),
            PropertyType::MultiSelect => Value::ids(p.options().iter().map(|o| o.id.clone())),
            PropertyType::Select => match p.options().last() {
                Some(o) => Value::text(o.id.clone()),
                None => continue,
            },
            PropertyType::Checkbox => Value::Bool(true),
            PropertyType::Person => Value::People(vec![
                Person::Owner,
                Person::agent("career-planning"),
                Person::agent("inbox-triage"),
            ]),
            PropertyType::Relation => Value::ids([
                "9d1de2a0-5d15-4a2f-b2ee-4e4519f9919c",
                "895fd7b3-26b9-4c7f-9118-bb16000f49c8",
            ]),
            PropertyType::Date => Value::Date(DateValue {
                start: NOW,
                end: Some(NOW + 40 * 86_400_000),
                include_time: true,
            }),
            _ => continue,
        };
        values.insert(p.id.clone(), v);
    }
    let mut row = Doc::row(
        "0f0f0f0f-0000-4000-8000-000000000001",
        db.doc.id.clone(),
        format!("{word}{word} 日本語のとても長い名前 العربية 🎓"),
        "Z",
        values,
    );
    row.created = NOW;
    row.updated = NOW;
    row
}

/// A bare one-line field for a typed value: characters, Backspace, Enter and Escape. No IME,
/// no selection; it stands in until gpui-core has an input.
struct Field {
    edit: TextEdit,
    text: String,
}

struct Host {
    dbs: RefCell<Vec<Db>>,
    log: RefCell<Vec<String>>,
    crm: RefCell<Option<WeakEntity<Crm>>>,
}

impl Host {
    fn apply(&self, edit: &Edit) {
        let mut dbs = self.dbs.borrow_mut();
        let row = |dbs: &mut Vec<Db>, id: &str| -> Option<(usize, usize)> {
            dbs.iter()
                .enumerate()
                .find_map(|(d, db)| db.rows.iter().position(|r| r.id == id).map(|i| (d, i)))
        };
        match edit {
            Edit::RowCreated {
                row_id,
                database_id,
                order,
                title,
                values,
            } => {
                if let Some(db) = dbs.iter_mut().find(|d| d.doc.id == *database_id) {
                    let mut r = Doc::row(
                        row_id.clone(),
                        database_id.clone(),
                        title.clone(),
                        order.clone(),
                        values.clone(),
                    );
                    r.created = NOW;
                    r.updated = NOW;
                    r.author = Some(Person::Owner);
                    r.updated_by = Some(Person::Owner);
                    db.rows.push(r);
                }
            }
            Edit::RowTitleChanged { row_id, title } => {
                if let Some((d, i)) = row(&mut dbs, row_id) {
                    dbs[d].rows[i].title = title.clone();
                    dbs[d].rows[i].updated = NOW;
                }
            }
            Edit::RowValueChanged {
                row_id,
                property_id,
                value,
            } => {
                if let Some((d, i)) = row(&mut dbs, row_id) {
                    let r = &mut dbs[d].rows[i];
                    let values = r.values.get_or_insert_with(Values::new);
                    match value {
                        Some(v) => {
                            values.insert(property_id.clone(), v.clone());
                        }
                        None => {
                            values.remove(property_id);
                        }
                    }
                    r.updated = NOW;
                }
            }
            Edit::RelationChanged {
                database_id,
                back_property_id,
                ..
            } => {
                let Some(other) = dbs.iter().position(|d| d.doc.id == *database_id) else {
                    return;
                };
                let back = relation::back_edits(
                    edit,
                    |id| {
                        dbs[other].rows.iter().find(|r| r.id == id).map(|r| {
                            r.value(back_property_id)
                                .and_then(Value::as_ids)
                                .unwrap_or(&[])
                                .to_vec()
                        })
                    },
                    &Limits::default(),
                );
                drop(dbs);
                for e in &back.edits {
                    self.apply(e);
                }
            }
            Edit::RowsMoved { row_ids, orders } => {
                for (id, o) in row_ids.iter().zip(orders) {
                    if let Some((d, i)) = row(&mut dbs, id) {
                        dbs[d].rows[i].order = o.clone();
                    }
                }
            }
            Edit::RowsDeleted { row_ids } => {
                for id in row_ids {
                    if let Some((d, i)) = row(&mut dbs, id) {
                        dbs[d].rows[i].deleted = true;
                    }
                }
            }
            Edit::ViewChanged { database_id, view } | Edit::ViewCreated { database_id, view } => {
                if let Some(fields) = dbs
                    .iter_mut()
                    .find(|d| d.doc.id == *database_id)
                    .and_then(|d| d.doc.database.as_mut())
                {
                    match fields.views.iter_mut().find(|v| v.id == view.id) {
                        Some(v) => *v = view.clone(),
                        None => fields.views.push(view.clone()),
                    }
                }
            }
            _ => {}
        }
    }
}

impl TableHost for Host {
    fn theme(&self, cx: &App) -> TableTheme {
        TableTheme::from_core(cx.theme())
    }

    fn event(&self, event: TableEvent, _: &mut Window, cx: &mut App) {
        self.log
            .borrow_mut()
            .push(serde_json::to_string(&event).unwrap_or_default());
        if let TableEvent::Edits { edits } = &event {
            for e in edits {
                self.apply(e);
            }
        }
        if let Some(crm) = self.crm.borrow().as_ref().and_then(WeakEntity::upgrade) {
            crm.update(cx, |crm, cx| crm.refresh(cx));
        }
    }

    fn person_name(&self, person: &Person) -> Option<String> {
        Some(match person {
            Person::Owner => "Avery Lin".into(),
            Person::Agent(id) if id == "career-planning" => "Career Planning".into(),
            Person::Agent(id) if id == "inbox-triage" => "Inbox Triage".into(),
            Person::Agent(id) => id.clone(),
            Person::Other(_) => return None,
        })
    }

    fn row_title(&self, database_id: &str, row_id: &str) -> Option<String> {
        let dbs = self.dbs.borrow();
        let db = dbs.iter().find(|d| d.doc.id == database_id)?;
        db.rows
            .iter()
            .find(|r| r.id == row_id)
            .map(|r| r.title.clone())
    }

    fn people(&self, _: &Property) -> Vec<Person> {
        vec![
            Person::Owner,
            Person::agent("career-planning"),
            Person::agent("inbox-triage"),
        ]
    }

    fn relation_rows(&self, database_id: &str) -> Vec<RelatedRow> {
        let dbs = self.dbs.borrow();
        let Some(db) = dbs.iter().find(|d| d.doc.id == database_id) else {
            return Vec::new();
        };
        let mut rows: Vec<RelatedRow> = db
            .rows
            .iter()
            .filter(|r| !r.deleted)
            .map(|r| RelatedRow {
                id: r.id.clone(),
                title: r.title.clone(),
            })
            .collect();
        rows.sort_by_key(|r| r.title.to_lowercase());
        rows
    }

    fn edit_text(&self, edit: TextEdit, window: &mut Window, cx: &mut App) {
        if let Some(crm) = self.crm.borrow().as_ref().and_then(WeakEntity::upgrade) {
            crm.update(cx, |crm, cx| {
                crm.field = Some(Field {
                    text: edit.text.clone(),
                    edit,
                });
                window.focus(&crm.field_focus, cx);
                cx.notify();
            });
        }
    }

    fn now(&self) -> i64 {
        NOW
    }
}

struct Crm {
    host: Rc<Host>,
    tables: Vec<Entity<TableView>>,
    current: Rc<Cell<usize>>,
    field: Option<Field>,
    field_focus: FocusHandle,
}

impl Crm {
    fn new(host: Rc<Host>, cx: &mut Context<Self>) -> Self {
        let tables = (0..host.dbs.borrow().len())
            .map(|i| {
                let (doc, rows) = {
                    let dbs = host.dbs.borrow();
                    (dbs[i].doc.clone(), dbs[i].rows.clone())
                };
                let h: Rc<dyn TableHost> = host.clone();
                cx.new(|cx| TableView::new(h, doc, rows, cx))
            })
            .collect();
        Self {
            host,
            tables,
            current: Rc::new(Cell::new(0)),
            field: None,
            field_focus: cx.focus_handle(),
        }
    }

    /// Hands every table its docs again, after the host applied edits.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        for (i, table) in self.tables.iter().enumerate() {
            let (doc, rows) = {
                let dbs = self.host.dbs.borrow();
                (dbs[i].doc.clone(), dbs[i].rows.clone())
            };
            table.update(cx, |t, cx| t.set_docs(doc, rows, cx));
        }
        cx.notify();
    }

    fn table(&self) -> Entity<TableView> {
        self.tables[self.current.get()].clone()
    }

    fn finish_field(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = self.field.take() {
            if commit {
                f.edit.reply.commit(&f.text, window, cx);
            } else {
                f.edit.reply.cancel(window, cx);
            }
            cx.notify();
        }
    }

    fn render_field(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let f = self.field.as_ref()?;
        let theme = cx.theme();
        let b = f.edit.bounds;
        let empty = f.text.is_empty();
        Some(
            deferred(
                anchored().position(b.origin).snap_to_window().child(
                    div()
                        .id("field")
                        .track_focus(&self.field_focus)
                        .occlude()
                        .w(b.size.width.max(px(220.)))
                        .h(b.size.height)
                        .px_2()
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .bg(theme.popover)
                        .border_2()
                        .border_color(theme.ring)
                        .text_sm()
                        .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                            let k = &e.keystroke;
                            match k.key.as_str() {
                                "enter" => this.finish_field(true, window, cx),
                                "escape" => this.finish_field(false, window, cx),
                                "backspace" => {
                                    if let Some(f) = &mut this.field {
                                        f.text.pop();
                                    }
                                }
                                _ => {
                                    if let (Some(ch), Some(f)) = (&k.key_char, &mut this.field)
                                        && !k.modifiers.platform
                                        && !k.modifiers.control
                                    {
                                        let max = f.edit.max_chars.unwrap_or(usize::MAX);
                                        if f.text.chars().count() < max {
                                            f.text.push_str(ch);
                                        }
                                    }
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.finish_field(true, window, cx)
                        }))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .when(empty, |d| d.text_color(theme.muted_foreground))
                                .child(if empty {
                                    SharedString::from(f.edit.label.clone())
                                } else {
                                    SharedString::from(format!("{}▏", f.text))
                                }),
                        ),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

impl Render for Crm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let current = self.current.get();
        let table = self.table();
        let names: Vec<String> = self
            .host
            .dbs
            .borrow()
            .iter()
            .map(|d| d.doc.title.clone())
            .collect();
        let (views, active) = {
            let t = table.read(cx);
            let active = t.view().map(|v| v.id.clone());
            let views: Vec<(String, String)> = t
                .database()
                .database
                .as_ref()
                .map(|f| {
                    f.views_in_order()
                        .into_iter()
                        .map(|v| (v.id.clone(), v.name.clone()))
                        .collect()
                })
                .unwrap_or_default();
            (views, active)
        };
        let tab = |id: ElementId, label: String, on: bool| {
            Press::new(id)
                .selected(on)
                .role(Role::Tab)
                .px_3()
                .h_8()
                .flex()
                .items_center()
                .rounded_md()
                .text_sm()
                .when(on, |d| d.bg(theme.secondary).text_color(theme.foreground))
                .when(!on, |d| d.text_color(theme.muted_foreground))
                .hover(|s| s.bg(theme.accent))
                .child(div().truncate().child(label))
        };
        let log: Vec<String> = self
            .host
            .log
            .borrow()
            .iter()
            .rev()
            .take(60)
            .cloned()
            .collect();
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .children(names.into_iter().enumerate().map(|(i, name)| {
                                tab(
                                    ElementId::NamedInteger("db".into(), i as u64),
                                    name,
                                    i == current,
                                )
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| {
                                        this.current.set(i);
                                        cx.notify();
                                    },
                                ))
                            }))
                            .child(div().w_px().h_5().mx_2().bg(theme.border))
                            .children(views.into_iter().enumerate().map(|(i, (id, name))| {
                                let on = active.as_deref() == Some(id.as_str());
                                let table = table.clone();
                                tab(ElementId::NamedInteger("view".into(), i as u64), name, on)
                                    .on_click(move |_: &ClickEvent, _, cx: &mut App| {
                                        table.update(cx, |t, cx| t.set_view(id.clone(), cx));
                                    })
                            })),
                    )
                    .child(div().flex_1().min_h_0().child(table)),
            )
            .child(
                div()
                    .id("log")
                    .flex_none()
                    .w(px(340.))
                    .h_full()
                    .border_l_1()
                    .border_color(theme.border)
                    .bg(theme.sidebar)
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .border_b_1()
                            .border_color(theme.border)
                            .child("Events"),
                    )
                    .child(
                        div()
                            .id("log-lines")
                            .flex_1()
                            .overflow_y_scroll()
                            .px_3()
                            .py_2()
                            .font_family(theme.mono_font_family.clone())
                            .text_xs()
                            .children(log.into_iter().map(|l| {
                                div()
                                    .py_1()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .line_clamp(4)
                                    .text_color(theme.muted_foreground)
                                    .child(l)
                            })),
                    ),
            )
            .children(self.render_field(cx))
    }
}

actions!(crm_example, [Quit]);

fn main() {
    let args = args();
    gpui_platform::application()
        .with_assets(gpui_core::Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            gpui_tables::view::init(cx);
            Theme::set(
                if args.scene.ends_with("light") {
                    Theme::light()
                } else {
                    Theme::dark()
                },
                cx,
            );
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());

            let mut dbs = vec![load(PEOPLE), load(COMPANIES), load(INTERACTIONS)];
            if args.scene.ends_with("worst") {
                let row = worst_row(&dbs[0]);
                dbs[0].rows.push(row);
            }
            let host = Rc::new(Host {
                dbs: RefCell::new(dbs),
                log: RefCell::default(),
                crm: RefCell::default(),
            });
            let bounds = Bounds::centered(None, size(px(1280.), px(760.)), cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("CRM".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // A picture doesn't take the keyboard or come forward, and isn't shown at all.
                focus: args.shot.is_none(),
                show: args.shot.is_none(),
                ..Default::default()
            };
            let (window, crm) = gpui_core::open_window(options, cx, {
                let host = host.clone();
                move |_, cx| cx.new(|cx| Crm::new(host, cx))
            })
            .expect("the window");
            *host.crm.borrow_mut() = Some(crm.downgrade());
            if args.shot.is_none() {
                cx.activate(true);
                let table = crm.read(cx).table();
                let _ = cx.update_window(window, |_, window, cx| {
                    table.focus_handle(cx).focus(window, cx)
                });
            }
            #[cfg(feature = "shots")]
            if let Some(path) = args.shot.clone() {
                shots::shot(window, crm.clone(), path, args.scene.clone(), cx);
            }
            let _ = (window, crm, &args.scene);
        });
}

#[cfg(feature = "shots")]
mod shots {
    use super::*;
    use std::time::Duration;

    async fn wait(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }

    /// Draws the window now, more than once: a window that isn't shown gets no frames from the
    /// display, and the table lays its menus out from what the frame before measured.
    async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
        for _ in 0..3 {
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            wait(cx, 80).await;
        }
    }

    const STAGE: &str = "6b4cb242-4a23-4596-a217-beaddbc496cb";
    const PIPELINE: &str = "f3aed0b6-c7ac-4491-9ef8-8334e647cb8f";
    const LAST_CONTACTED: &str = "ae97ba94-d0ed-482f-8f6d-05584ef8aa38";
    const IMANI: &str = "1d87cec3-1f72-46ab-b961-fd925d39d0a8";
    const FOLLOW_UP: &str = "923a7369-94e3-4f91-9a61-dbe22e44158b";

    /// The database's first board view.
    fn board_view(t: &Entity<TableView>, cx: &App) -> String {
        t.read(cx)
            .database()
            .database
            .as_ref()
            .and_then(|f| f.views.iter().find(|v| v.kind == ViewType::Board))
            .map(|v| v.id.clone())
            .unwrap_or_default()
    }

    /// Picks the first card of the board's second column up with the mouse and carries it over
    /// the third column, under its first card, without letting go.
    async fn drag_a_card(window: AnyWindowHandle, crm: &Entity<Crm>, cx: &mut AsyncApp) {
        let points = cx.update(|cx| {
            let t = crm.read(cx).table();
            t.update(cx, |t, _| {
                let cols = t.board_columns().to_vec();
                let card = |c: usize, i: usize| {
                    cols.get(c)
                        .and_then(|c| c.rows.get(i))
                        .and_then(|&r| t.card_bounds(&t.rows()[r].id))
                };
                let from = card(1, 0)?;
                let over = card(2, 0)?;
                let start = point(from.left() + px(40.), from.top() + px(18.));
                let end = point(over.left() + px(56.), over.bottom() - px(14.));
                Some((start, end))
            })
        });
        let Some((start, end)) = points else {
            return;
        };
        let send = |event: PlatformInput, cx: &mut AsyncApp| {
            let _ = cx.update_window(window, |_, window, cx| {
                window.dispatch_event(event, cx);
                window.refresh();
                let _ = window.draw(cx);
            });
        };
        send(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: start,
                pressed_button: None,
                modifiers: Modifiers::none(),
            }),
            cx,
        );
        send(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        for i in 1..=6 {
            let at = start + (end - start) * (i as f32 / 6.);
            send(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: at,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::none(),
                }),
                cx,
            );
            wait(cx, 30).await;
        }
        settle(window, cx).await;
    }

    /// A picture of the window, rendered offscreen, then quits.
    pub fn shot(
        window: AnyWindowHandle,
        crm: Entity<Crm>,
        path: PathBuf,
        scene: String,
        cx: &mut App,
    ) {
        cx.spawn(async move |cx| {
            wait(cx, 300).await;
            settle(window, cx).await;
            let _ = cx.update_window(window, |_, window, cx| {
                let pick = |i: usize, cx: &mut App| {
                    crm.update(cx, |c, cx| {
                        c.current.set(i);
                        cx.notify();
                    });
                    crm.read(cx).table()
                };
                match scene.as_str() {
                    "grouped" => {
                        let _ = pick(2, cx);
                    }
                    "light" => {
                        let _ = pick(1, cx);
                    }
                    "menu" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.open_column_menu(STAGE, cx));
                    }
                    "picker" => {
                        let t = pick(0, cx);
                        // A checkbox ticked first, so the log has an event in it.
                        t.update(cx, |t, cx| t.edit(IMANI, FOLLOW_UP, window, cx));
                        t.update(cx, |t, cx| t.edit(IMANI, STAGE, window, cx));
                    }
                    "date" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.edit(IMANI, LAST_CONTACTED, window, cx));
                    }
                    "board" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.set_view(PIPELINE, cx));
                    }
                    "board-worst" => {
                        // The worst row is Dormant, folded in the fixture: unfolded, and the
                        // board scrolled to it.
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| {
                            t.set_view(PIPELINE, cx);
                            if let Some(c) = t.board_columns().iter().position(|c| c.folded) {
                                t.toggle_column(c, window, cx);
                            }
                        });
                    }
                    "board-light" => {
                        let t = pick(1, cx);
                        let id = board_view(&t, cx);
                        t.update(cx, |t, cx| t.set_view(id, cx));
                    }
                    "board-drag" => {
                        let t = pick(2, cx);
                        let id = board_view(&t, cx);
                        t.update(cx, |t, cx| t.set_view(id, cx));
                    }
                    "scrolled" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| {
                            t.focus_cell(IMANI, STAGE, cx);
                            t.set_scroll_x(px(420.), cx)
                        });
                    }
                    _ => {}
                }
            });
            settle(window, cx).await;
            if scene == "board-drag" {
                drag_a_card(window, &crm, cx).await;
            }
            if scene == "board-worst" {
                cx.update(|cx| {
                    let t = crm.read(cx).table();
                    t.update(cx, |t, cx| t.set_board_scroll_x(px(100_000.), cx));
                });
                settle(window, cx).await;
            }
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
                match window.render_to_image() {
                    Ok(img) => {
                        if let Err(e) = img.save(&path) {
                            eprintln!("the picture wasn't saved: {e}");
                        }
                    }
                    Err(e) => eprintln!("the window didn't render to a picture: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
    }
}
