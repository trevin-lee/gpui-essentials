//! What the view tests share: a host that records what the table hands it, the CRM fixtures,
//! a table in a window, and the mouse and keyboard to drive it.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::*;
use gpui_tables::view::{RelatedDatabase, RelatedRow, TableHost, TableTheme, TableView, TextEdit};
use gpui_tables::*;
use gpui_tables::{Edit, View};

/// The fixtures' "now": 2026-11-15, noon UTC.
pub const NOW: i64 = 1_794_744_000_000;

pub mod ids {
    pub const PEOPLE: &str = "36f675cc-81e7-4ef5-a8e2-5d940ed90475";
    pub const ALL_PEOPLE: &str = "74e69a5d-0dd2-4a65-bd62-8881ad1b72db";
    pub const NAME: &str = "90c192cf-d3ac-44af-8f21-ddb66cad4a26";
    pub const COMPANY: &str = "a170b338-3926-4059-b28c-105d1fb17c23";
    pub const ROLE: &str = "0fd630f1-f29d-4da9-953f-48f1a09f76b5";
    pub const EMAIL: &str = "0cb1e29c-658c-4a14-95e6-0af593bd04cf";
    pub const PHONE: &str = "8e81973e-0bec-47b0-b898-d190f9ebdacc";
    pub const STAGE: &str = "6b4cb242-4a23-4596-a217-beaddbc496cb";
    pub const TAGS: &str = "92276658-1e27-41c0-8a6a-63ec24ede6a4";
    pub const LAST_CONTACTED: &str = "ae97ba94-d0ed-482f-8f6d-05584ef8aa38";
    pub const FOLLOW_UP: &str = "923a7369-94e3-4f91-9a61-dbe22e44158b";
    pub const OWNER: &str = "18f135d2-5f55-4203-b018-50c5a38fd547";
    pub const PROFILE: &str = "907a70c3-1012-4037-b64c-e4228c38fb29";
    pub const PIPELINE: &str = "f3aed0b6-c7ac-4491-9ef8-8334e647cb8f";
    pub const LEAD: &str = "5affb229-7631-4992-b0ce-583505c6af07";
    pub const WARM: &str = "7e62aa0a-1df9-4d78-9c65-39382b0537e6";
    pub const ACTIVE: &str = "49952399-c4aa-4ac1-b7dc-76fb0f17a300";
    pub const DORMANT: &str = "65dc9f50-3f63-4f83-bd05-61e6211c70cf";
    pub const MENTOR: &str = "7f1b103c-df15-42b0-aab4-77d26415479c";
    pub const INVESTOR: &str = "66d22876-72fd-4202-aa96-fb1a14a0f9e7";
    pub const WARMTH: &str = "c6f87718-6d76-407e-881e-d162ae2eb154";
    pub const ROWAN: &str = "9a2ef80f-58ee-4571-b499-8d7c4093f6de";
    pub const IMANI: &str = "1d87cec3-1f72-46ab-b961-fd925d39d0a8";
    pub const THEO: &str = "fa529ba3-fe3b-4ada-bcf2-0724d953ee26";
    pub const ANA: &str = "f3b7a50d-f373-4a53-b488-f87605e999f3";
    pub const KESTREL: &str = "895fd7b3-26b9-4c7f-9118-bb16000f49c8";
    pub const BLUEFIN: &str = "9d1de2a0-5d15-4a2f-b2ee-4e4519f9919c";
    pub const COMPANIES: &str = "6b0d549b-6f03-475a-9600-a35a099950d8";
}

/// A database and its rows, from a fixture.
pub fn load(name: &str) -> (Doc, Vec<Doc>) {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let docs: Vec<Doc> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (dbs, rows): (Vec<Doc>, Vec<Doc>) =
        docs.into_iter().partition(|d| d.kind == DocKind::Database);
    (dbs.into_iter().next().unwrap(), rows)
}

/// The database with the view `view` drawn as a table (a board view drawn as a grouped table).
pub fn as_table(mut db: Doc, view: &str) -> Doc {
    if let Some(v) = db
        .database
        .as_mut()
        .and_then(|f| f.views.iter_mut().find(|v| v.id == view))
    {
        v.kind = ViewType::Table;
    }
    db
}

/// A property's id by its name.
pub fn prop(db: &Doc, name: &str) -> String {
    db.database
        .as_ref()
        .unwrap()
        .properties
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no property {name}"))
        .id
        .clone()
}

/// A view's id by its name.
pub fn view_id(db: &Doc, name: &str) -> String {
    db.database
        .as_ref()
        .unwrap()
        .views
        .iter()
        .find(|v| v.name == name)
        .unwrap()
        .id
        .clone()
}

pub struct TestHost {
    pub events: RefCell<Vec<TableEvent>>,
    /// Every typed value the table asked for, whoever edits it.
    pub texts: RefCell<Vec<TextEdit>>,
    /// Whether the host shows its own field for typed values (overriding
    /// `TableHost::edit_text`), else hands them back to be edited in place.
    pub hook: Cell<bool>,
    /// Times the table drew (it asks for its theme once a draw).
    pub draws: Cell<usize>,
    pub theme: RefCell<TableTheme>,
    /// Applies edits and hands the table its docs back, as a real host does.
    pub apply: Cell<bool>,
    pub table: RefCell<Option<WeakEntity<TableView>>>,
    pub companies: Vec<Doc>,
    /// The other databases a relation can point at (Companies by default).
    pub databases: RefCell<Vec<RelatedDatabase>>,
    /// The limits the table is held to.
    pub limits: Cell<Limits>,
    /// The views the table said it showed, as a person switched them.
    pub shown: RefCell<Vec<String>>,
}

impl TestHost {
    pub fn new() -> Rc<TestHost> {
        Rc::new(TestHost {
            events: RefCell::default(),
            texts: RefCell::default(),
            hook: Cell::new(false),
            draws: Cell::new(0),
            theme: RefCell::new(TableTheme::dark().with_touch(false)),
            apply: Cell::new(true),
            table: RefCell::default(),
            companies: load("companies").1,
            databases: RefCell::new(vec![related(&load("companies").0)]),
            limits: Cell::new(Limits::default()),
            shown: RefCell::default(),
        })
    }

    /// A host with a field of its own: typed values come to it, answered through their reply.
    pub fn with_hook() -> Rc<TestHost> {
        let host = TestHost::new();
        host.hook.set(true);
        host
    }

    pub fn touch() -> Rc<TestHost> {
        let host = TestHost::new();
        let t = host.theme.borrow().clone().with_touch(true);
        *host.theme.borrow_mut() = t;
        host
    }

    /// Every edit emitted so far, in order.
    pub fn edits(&self) -> Vec<Edit> {
        self.events
            .borrow()
            .iter()
            .flat_map(|e| match e {
                TableEvent::Edits { edits } => edits.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    pub fn last_edit(&self) -> Edit {
        self.edits().pop().expect("an edit")
    }

    /// The last viewChanged's view.
    pub fn last_view(&self) -> View {
        self.edits()
            .into_iter()
            .rev()
            .find_map(|e| match e {
                Edit::ViewChanged { view, .. } => Some(view),
                _ => None,
            })
            .expect("a viewChanged")
    }

    pub fn clear(&self) {
        self.events.borrow_mut().clear();
    }
}

/// A database as a relation's menu lists it.
pub fn related(db: &Doc) -> RelatedDatabase {
    RelatedDatabase {
        id: db.id.clone(),
        title: db.title.clone(),
        properties: db
            .database
            .as_ref()
            .map(|f| f.properties.clone())
            .unwrap_or_default(),
    }
}

fn apply(db: &mut Doc, rows: &mut Vec<Doc>, edit: &Edit) {
    match edit {
        Edit::PropertyCreated {
            database_id,
            property,
        } if *database_id == db.id => {
            let f = db.database.as_mut().unwrap();
            match f.properties.iter_mut().find(|p| p.id == property.id) {
                Some(p) => *p = property.clone(),
                None => f.properties.push(property.clone()),
            }
        }
        Edit::PropertyDeleted {
            database_id,
            property_id,
        } if *database_id == db.id => {
            let f = db.database.as_mut().unwrap();
            f.properties.retain(|p| p.id != *property_id);
        }
        Edit::RowCreated {
            row_id,
            database_id,
            order,
            title,
            values,
        } => rows.push(Doc::row(
            row_id.clone(),
            database_id.clone(),
            title.clone(),
            order.clone(),
            values.clone(),
        )),
        Edit::RowTitleChanged { row_id, title } => {
            if let Some(r) = rows.iter_mut().find(|r| r.id == *row_id) {
                r.title = title.clone();
            }
        }
        Edit::RowValueChanged {
            row_id,
            property_id,
            value,
        } => {
            if let Some(r) = rows.iter_mut().find(|r| r.id == *row_id) {
                let v = r.values.get_or_insert_with(Values::new);
                match value {
                    Some(x) => {
                        v.insert(property_id.clone(), x.clone());
                    }
                    None => {
                        v.remove(property_id);
                    }
                }
            }
        }
        Edit::RowsMoved { row_ids, orders } => {
            for (id, o) in row_ids.iter().zip(orders) {
                if let Some(r) = rows.iter_mut().find(|r| r.id == *id) {
                    r.order = o.clone();
                }
            }
        }
        Edit::PropertyChanged {
            property,
            database_id,
        } if *database_id == db.id => {
            let f = db.database.as_mut().unwrap();
            if let Some(p) = f.properties.iter_mut().find(|p| p.id == property.id) {
                *p = property.clone();
            }
        }
        Edit::ViewChanged { view, .. } => {
            let f = db.database.as_mut().unwrap();
            if let Some(v) = f.views.iter_mut().find(|v| v.id == view.id) {
                *v = view.clone();
            }
        }
        Edit::ViewCreated { database_id, view } if *database_id == db.id => {
            let f = db.database.as_mut().unwrap();
            match f.views.iter_mut().find(|v| v.id == view.id) {
                Some(v) => *v = view.clone(),
                None => f.views.push(view.clone()),
            }
        }
        Edit::ViewDeleted {
            database_id,
            view_id,
        } if *database_id == db.id => {
            let f = db.database.as_mut().unwrap();
            f.views.retain(|v| v.id != *view_id);
        }
        _ => {}
    }
}

impl TableHost for TestHost {
    fn theme(&self, _: &App) -> TableTheme {
        self.draws.set(self.draws.get() + 1);
        self.theme.borrow().clone()
    }

    fn event(&self, event: TableEvent, _: &mut Window, cx: &mut App) {
        self.events.borrow_mut().push(event.clone());
        if !self.apply.get() {
            return;
        }
        let Some(table) = self.table.borrow().as_ref().and_then(WeakEntity::upgrade) else {
            return;
        };
        if let TableEvent::Edits { edits } = event {
            let (mut db, mut rows) = {
                let t = table.read(cx);
                (t.database().clone(), t.rows().to_vec())
            };
            for e in &edits {
                apply(&mut db, &mut rows, e);
            }
            table.update(cx, |t, cx| t.set_docs(db, rows, cx));
        }
    }

    fn person_name(&self, person: &Person) -> Option<String> {
        Some(match person {
            Person::Owner => "Avery Lin".into(),
            Person::Agent(id) if id == "career-planning" => "Career Planning".into(),
            Person::Agent(id) => id.clone(),
            Person::Other(_) => return None,
        })
    }

    fn row_title(&self, _: &str, row_id: &str) -> Option<String> {
        self.companies
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
        if database_id != ids::COMPANIES {
            return Vec::new();
        }
        self.companies
            .iter()
            .map(|r| RelatedRow {
                id: r.id.clone(),
                title: r.title.clone(),
            })
            .collect()
    }

    fn edit_text(&self, edit: TextEdit, window: &mut Window, cx: &mut App) {
        if !self.hook.get() {
            edit.reply.edit_in_place(window, cx);
        }
        self.texts.borrow_mut().push(edit);
    }

    fn now(&self) -> i64 {
        NOW
    }

    fn databases(&self) -> Vec<RelatedDatabase> {
        self.databases.borrow().clone()
    }

    fn limits(&self) -> Limits {
        self.limits.get()
    }

    fn view_shown(&self, view_id: &str, _: &mut Window, _: &mut App) {
        self.shown.borrow_mut().push(view_id.to_owned());
    }
}

pub fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_tables::view::init(cx);
    });
}

/// A table of `db` in a window `size` big, inside gpui-core's Root (so the rem is 15 points),
/// focused and drawn.
pub fn open_with<'a>(
    cx: &'a mut TestAppContext,
    host: &Rc<TestHost>,
    db: Doc,
    rows: Vec<Doc>,
    view: Option<&str>,
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    setup(cx);
    let h: Rc<dyn TableHost> = host.clone();
    let made: Rc<RefCell<Option<Entity<TableView>>>> = Rc::default();
    let view = view.map(str::to_owned);
    let (_, vcx) = cx.add_window_view({
        let made = made.clone();
        move |window, cx| {
            let table = cx.new(|cx| {
                let t = TableView::new(h, db, rows, cx);
                match view {
                    Some(v) => t.with_view(v),
                    None => t,
                }
            });
            *made.borrow_mut() = Some(table.clone());
            gpui_core::Root::new(table, window, cx)
        }
    });
    let table = made.borrow().clone().unwrap();
    *host.table.borrow_mut() = Some(table.downgrade());
    vcx.simulate_resize(size);
    vcx.update(|window, cx| table.read(cx).focus_handle(cx).focus(window, cx));
    draw(vcx);
    draw(vcx);
    (table, vcx)
}

/// The People database's "All people" table at 2600 by 700, wide enough for every column.
pub fn open_people<'a>(
    cx: &'a mut TestAppContext,
    host: &Rc<TestHost>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (db, rows) = load("people");
    open_with(cx, host, db, rows, None, size(px(2600.), px(700.)))
}

/// Brings the window to the front, as a person using it has it: a field loses the focus (and
/// commits) only in an active window.
pub fn activate(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    draw(cx);
}

pub fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

pub fn sel(s: impl Into<String>) -> &'static str {
    Box::leak(s.into().into_boxed_str())
}

pub fn rect(cx: &mut VisualTestContext, what: impl Into<String>) -> Bounds<Pixels> {
    let what = what.into();
    cx.debug_bounds(sel(what.clone()))
        .unwrap_or_else(|| panic!("{what} isn't drawn"))
}

pub fn drawn(cx: &mut VisualTestContext, what: impl Into<String>) -> bool {
    cx.debug_bounds(sel(what)).is_some()
}

pub fn click(cx: &mut VisualTestContext, what: impl Into<String>) {
    let at = rect(cx, what).center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
}

pub fn click_with(cx: &mut VisualTestContext, what: impl Into<String>, modifiers: Modifiers) {
    let at = rect(cx, what).center();
    cx.simulate_click(at, modifiers);
    draw(cx);
}

pub fn cell(row: &str, property: &str) -> String {
    format!("cell:{row}:{property}")
}

pub fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

/// Types text into the focused field, through the platform's input handler as a keyboard or
/// an input method would.
pub fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    draw(cx);
}

/// What's typed in place, and what for, while a field is open.
pub fn editing(
    table: &Entity<TableView>,
    cx: &mut VisualTestContext,
) -> Option<(gpui_tables::view::TextTarget, String)> {
    table.update(cx, |t, cx| t.editing(cx))
}

/// Selects all the text in the field open in place, so typing replaces it.
pub fn select_all(table: &Entity<TableView>, cx: &mut VisualTestContext) {
    let state = table
        .read_with(cx, |t, _| t.editor_state())
        .expect("a field open in place");
    state.update(cx, |s, cx| {
        let n = s.value().len();
        s.select(0..n, cx)
    });
    draw(cx);
}

pub fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    let mid = point((from.x + to.x) / 2., (from.y + to.y) / 2.);
    cx.simulate_mouse_move(mid, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

pub fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
}

/// A finger dragged `by` from `from` in steps a frame apart, held, then lifted (no fling).
pub fn pan(cx: &mut VisualTestContext, from: Point<Pixels>, by: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    let steps = 8;
    for i in 1..=steps {
        cx.executor().advance_clock(Duration::from_millis(16));
        touch(cx, TouchPhase::Moved, from + by * (i as f32 / steps as f32));
    }
    cx.executor().advance_clock(Duration::from_millis(200));
    touch(cx, TouchPhase::Ended, from + by);
    draw(cx);
}

pub fn tap(cx: &mut VisualTestContext, at: Point<Pixels>) {
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Ended, at);
    draw(cx);
}

/// A finger pressed and held at `from` until it's a long press, then moved to `to` a frame at a
/// time, held still and lifted.
pub fn long_press(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    cx.executor().advance_clock(Duration::from_millis(800));
    cx.run_until_parked();
    draw(cx);
    let steps = 8;
    for i in 1..=steps {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        touch(
            cx,
            TouchPhase::Moved,
            from + (to - from) * (i as f32 / steps as f32),
        );
        draw(cx);
    }
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    touch(cx, TouchPhase::Ended, to);
    draw(cx);
}
