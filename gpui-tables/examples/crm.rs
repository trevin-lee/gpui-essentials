//! The CRM in the fixtures (People, Companies and Interactions, all made up) in a window, with
//! its table, board and list views and a log of every event they emit.
//!
//! ```sh
//! cargo run --release -p gpui-tables --example crm
//! cargo run --release -p gpui-tables --features shots --example crm -- --shot out.png [--scene menu]
//! ```
//!
//! The host here keeps the docs in memory and applies the edits itself (both sides of a two-way
//! relation too, and a new two-way relation's other side in the other database), so the table
//! changes as you use it, properties added and changed from the + and the columns' menus too,
//! and views added, renamed, duplicated, retyped, reordered and deleted from the crate's bar of
//! views over each database (`TableView::with_view_bar`). Typed values (titles, text, numbers,
//! emails...) are edited in place, in gpui-core's `Input`: the host doesn't implement
//! `TableHost::edit_text`.
//!
//! `--shot` takes a picture of the window without showing it. Scenes: `people` (the default),
//! `grouped` (Interactions by month), `menu` (a column's menu, its types open beside it),
//! `picker` (a select's options),
//! `date` (a date's month, today ringed), `edit` (a text value typed in place, grown over the rows below),
//! `scrolled` (scrolled sideways, the title column in place), `light` and `worst` (worst-case
//! text, every cell ending in an ellipsis); and boards: `board` (People's
//! Pipeline by stage), `board-drag` (a card of Interactions by kind dragged to another column),
//! `board-light` (Companies by industry, in the light theme) and `board-worst`; and lists:
//! `list` (Interactions, most recent first), `list-grouped` (People by stage) and `list-light`
//! (the Companies directory, in the light theme); and properties: `columns` (the + menu, a new
//! property named, its type to pick) and `options` (a select's options beside its column's
//! menu, the first one's submenu open to rename, recolour, reorder and delete it); and views:
//! `add-view` (the bar's + menu, a new view named, its type to pick), `view-menu` (a view's
//! menu) and `view-settings` (a board's settings beside its menu, its card properties open).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Press, Selectable as _, Theme};
use gpui_tables::engine::relation;
use gpui_tables::view::{RelatedDatabase, RelatedRow, TableHost, TableTheme, TableView};
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
                        dbs[d].rows[i].deleted = Some(NOW);
                    }
                }
            }
            // A property changed whole: an option made in a select's picker, say.
            Edit::PropertyChanged {
                database_id,
                property,
            }
            | Edit::PropertyCreated {
                database_id,
                property,
            } => {
                if let Some(fields) = dbs
                    .iter_mut()
                    .find(|d| d.doc.id == *database_id)
                    .and_then(|d| d.doc.database.as_mut())
                {
                    match fields.properties.iter_mut().find(|p| p.id == property.id) {
                        Some(p) => *p = property.clone(),
                        None => fields.properties.push(property.clone()),
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
            // A property removed: its rows' values for it may stay (the views ignore them), and
            // so may views' references to it, as the README says.
            Edit::PropertyDeleted {
                database_id,
                property_id,
            } => {
                if let Some(fields) = dbs
                    .iter_mut()
                    .find(|d| d.doc.id == *database_id)
                    .and_then(|d| d.doc.database.as_mut())
                {
                    fields.properties.retain(|p| p.id != *property_id);
                }
            }
            Edit::ViewDeleted {
                database_id,
                view_id,
            } => {
                if let Some(fields) = dbs
                    .iter_mut()
                    .find(|d| d.doc.id == *database_id)
                    .and_then(|d| d.doc.database.as_mut())
                {
                    fields.views.retain(|v| v.id != *view_id);
                }
            }
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
            .filter(|r| !r.is_deleted())
            .map(|r| RelatedRow {
                id: r.id.clone(),
                title: r.title.clone(),
            })
            .collect();
        rows.sort_by_key(|r| r.title.to_lowercase());
        rows
    }

    fn databases(&self) -> Vec<RelatedDatabase> {
        self.dbs
            .borrow()
            .iter()
            .map(|d| RelatedDatabase {
                id: d.doc.id.clone(),
                title: d.doc.title.clone(),
                properties: d
                    .doc
                    .database
                    .as_ref()
                    .map(|f| f.properties.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn now(&self) -> i64 {
        NOW
    }
}

struct Crm {
    host: Rc<Host>,
    tables: Vec<Entity<TableView>>,
    current: Rc<Cell<usize>>,
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
                cx.new(|cx| TableView::new(h, doc, rows, cx).with_view_bar(true))
            })
            .collect();
        Self {
            host,
            tables,
            current: Rc::new(Cell::new(0)),
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
    const ANA: &str = "f3b7a50d-f373-4a53-b488-f87605e999f3";
    const FOLLOW_UP: &str = "923a7369-94e3-4f91-9a61-dbe22e44158b";
    const ROLE: &str = "0fd630f1-f29d-4da9-953f-48f1a09f76b5";
    const THEO: &str = "fa529ba3-fe3b-4ada-bcf2-0724d953ee26";
    const SAMI: &str = "bfeaa155-1a28-47b3-a4e4-e25a15fc899e";

    /// People's table in the manual order (its sort taken off, in the table only), with Imani
    /// selected, and Theo too for `both`.
    fn unsorted_with_selection(t: &Entity<TableView>, both: bool, cx: &mut App) {
        t.update(cx, |t, cx| {
            let mut db = t.database().clone();
            if let Some(f) = db.database.as_mut() {
                for v in &mut f.views {
                    v.sort.clear();
                }
            }
            t.set_database(db, cx);
            t.select_row(IMANI, gpui_tables::view::Select::Only, cx);
            if both {
                t.select_row(THEO, gpui_tables::view::Select::Toggle, cx);
            }
        });
    }

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

    /// The database's view of this name.
    fn named_view(t: &Entity<TableView>, name: &str, cx: &App) -> String {
        t.read(cx)
            .database()
            .database
            .as_ref()
            .and_then(|f| f.views.iter().find(|v| v.name == name))
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
        if let Some((start, end)) = points {
            carry(window, start, end, cx).await;
        }
    }

    /// Picks Imani's row up by its handle (Theo's, selected too, with it) and carries them
    /// down between Lena and Sami, without letting go.
    async fn drag_rows(window: AnyWindowHandle, crm: &Entity<Crm>, cx: &mut AsyncApp) {
        let points = cx.update(|cx| {
            let t = crm.read(cx).table();
            let t = t.read(cx);
            let from = t.handle_bounds(IMANI)?;
            let over = t.handle_bounds(SAMI)?;
            Some((
                from.center(),
                point(over.left() + px(220.), over.top() + px(6.)),
            ))
        });
        if let Some((start, end)) = points {
            carry(window, start, end, cx).await;
        }
    }

    /// Presses the mouse at `start` and moves it to `end` in steps, without letting go.
    async fn carry(
        window: AnyWindowHandle,
        start: Point<Pixels>,
        end: Point<Pixels>,
        cx: &mut AsyncApp,
    ) {
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
                        // Ana's November 12, so the month shows today (the 15th) ringed.
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.edit(ANA, LAST_CONTACTED, window, cx));
                    }
                    "edit" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.edit(IMANI, ROLE, window, cx));
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
                    "list" => {
                        let t = pick(2, cx);
                        let id = named_view(&t, "Recent", cx);
                        t.update(cx, |t, cx| t.set_view(id, cx));
                    }
                    "list-grouped" => {
                        let t = pick(0, cx);
                        let id = named_view(&t, "By stage", cx);
                        t.update(cx, |t, cx| t.set_view(id, cx));
                    }
                    "list-light" => {
                        let t = pick(1, cx);
                        let id = named_view(&t, "Directory", cx);
                        t.update(cx, |t, cx| t.set_view(id, cx));
                    }
                    "columns" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| {
                            t.set_scroll_x(px(100_000.), cx);
                            t.open_add_property_menu(window, cx);
                        });
                    }
                    "options" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.open_column_menu(STAGE, cx));
                    }
                    "add-view" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| t.open_add_view_menu(window, cx));
                    }
                    "view-menu" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| {
                            t.open_view_menu(
                                PIPELINE,
                                gpui_tables::view::ViewMenu::Menu,
                                window,
                                cx,
                            )
                        });
                    }
                    "view-settings" => {
                        let t = pick(0, cx);
                        t.update(cx, |t, cx| {
                            t.set_view(PIPELINE, cx);
                            t.open_view_menu(
                                PIPELINE,
                                gpui_tables::view::ViewMenu::Menu,
                                window,
                                cx,
                            )
                        });
                    }
                    "rows" => {
                        let t = pick(0, cx);
                        unsorted_with_selection(&t, false, cx);
                        t.update(cx, |t, cx| t.open_row_menu(cx));
                    }
                    "rows-drag" => {
                        let t = pick(0, cx);
                        unsorted_with_selection(&t, true, cx);
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
            if scene == "edit" {
                // A second line typed (Shift-Enter makes one), so the field grows.
                let _ = cx.update_window(window, |_, window, cx| {
                    let t = crm.read(cx).table();
                    if let Some(state) = t.read(cx).editor_state() {
                        state.update(cx, |s, cx| {
                            let text = format!("{}\nAdvises two seed-stage founders", s.value());
                            s.set_value(text, window, cx)
                        });
                    }
                });
                settle(window, cx).await;
            }
            // Typed and pressed as a person does: a + menu's name, the keys that open submenus.
            let presses: &[&str] = match scene.as_str() {
                // The column's menu opens on Sort ascending: up to Change type, and open it.
                "menu" => &["up", "up", "right"],
                // Up to Edit options, open it, and open its first option.
                "options" => &["up", "right", "right"],
                // Down to View settings, open it, and open Card properties.
                "view-settings" => &["down", "down", "right", "down", "right"],
                // A new view's type: Board.
                "add-view" => &["down"],
                _ => &[],
            };
            let typed = match scene.as_str() {
                "columns" => "Deal size",
                "add-view" => "Follow-ups by owner",
                _ => "",
            };
            if !typed.is_empty() {
                let _ = cx.update_window(window, |_, window, cx| {
                    for ch in typed.chars() {
                        let key = match ch {
                            ' ' => "space".to_owned(),
                            c => c.to_lowercase().to_string(),
                        };
                        window.dispatch_keystroke(
                            Keystroke {
                                modifiers: Modifiers::none(),
                                key,
                                key_char: Some(ch.to_string()),
                            },
                            cx,
                        );
                    }
                });
                settle(window, cx).await;
            }
            // Each after the last has drawn, so a submenu just opened has the keys.
            for key in presses {
                let _ = cx.update_window(window, |_, window, cx| {
                    window.dispatch_keystroke(Keystroke::parse(key).expect("a key"), cx);
                });
                settle(window, cx).await;
            }
            if scene == "board-drag" {
                drag_a_card(window, &crm, cx).await;
            }
            if scene == "rows-drag" {
                drag_rows(window, &crm, cx).await;
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
