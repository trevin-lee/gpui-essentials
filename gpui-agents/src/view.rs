//! [`CardView`]: one card, as a GPUI entity. It owns the card's program, its state (the
//! [`Store`]), and what the drawing needs to keep between frames (the chosen tab, the text in
//! a field, an image the host sent); every change goes through the store and the card is read
//! again from it.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Autocapitalize, Context, Entity, EventEmitter, Image, SharedString,
    Subscription, TextInputAction, TextInputConfiguration, Window,
};
use gpui_core::{
    DatePickerEvent, DatePickerState, InputEvent, InputState, SliderEvent, SliderState,
};
use serde_json::{Value as Json, json};

use crate::draw;
use crate::host::{CardEvent, CardHost, ImageLoad, ImageReply, Limits};
use crate::openui::{
    self, Action, Bound, Card, CardError, Component, Effect, Field, ImageSrc, Input, InputKind,
    Program, Store,
};

/// One OpenUI card, drawn. Make it with [`CardView::new`] (from the program's text) or
/// [`CardView::from_program`], inside `cx.new(...)`.
pub struct CardView {
    pub(crate) id: SharedString,
    pub(crate) host: Rc<dyn CardHost>,
    program: Option<Program>,
    pub(crate) store: Store,
    pub(crate) card: Result<Card, CardError>,
    pub(crate) limits: Limits,
    pub(crate) images: HashMap<ImageSrc, ImageState>,
    pub(crate) ui: Ui,
    pub(crate) texts: HashMap<String, TextEntry>,
    pub(crate) sliders: HashMap<String, SliderEntry>,
    pub(crate) dates: HashMap<String, DateEntry>,
    pub(crate) edits: HashMap<String, EditTable>,
}

pub(crate) enum ImageState {
    Pending,
    Ready(Arc<Image>),
    Refused,
}

/// Where a field's value lives: its form (or none) and the field itself.
#[derive(Clone, Debug)]
pub(crate) struct Binding {
    pub form: Option<String>,
    pub field: Field,
}

/// A text field or text area: gpui-core's input, and where its text goes.
pub(crate) struct TextEntry {
    pub state: Entity<InputState>,
    pub binding: Binding,
    _subs: Vec<Subscription>,
}

pub(crate) struct SliderEntry {
    pub state: Entity<SliderState>,
    pub binding: Binding,
    _sub: Subscription,
}

pub(crate) struct DateEntry {
    pub state: Entity<DatePickerState>,
    pub binding: Binding,
    _sub: Subscription,
}

/// An editable table's rows as edited so far, and the cell being edited.
pub(crate) struct EditTable {
    pub rows: Vec<Vec<String>>,
    pub editing: Option<(usize, usize, Entity<InputState>, Subscription)>,
    pub dirty: bool,
}

/// A carousel's slide, and the one it's moving from.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Slide {
    pub index: usize,
    pub from: usize,
    pub moves: u32,
    /// Where a swipe that didn't turn the slide let go of it, in window pixels: the slide
    /// eases back from there.
    pub back: f32,
    pub swipe: Swipe,
}

/// A finger's pan over a carousel, as GPUI hands it over (scroll events from a touch).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Swipe {
    #[default]
    Idle,
    /// A finger is down and hasn't moved yet.
    Fresh,
    /// A sideways pan, this far (window pixels, left negative): the slide follows it.
    Dragging(f32),
    /// The momentum GPUI sends after a sideways pan lets go, kept from the chat around.
    Coasting,
    /// An up-and-down pan, left to the chat.
    Ignoring,
}

/// What the drawing remembers between frames.
#[derive(Default)]
pub(crate) struct Ui {
    pub tabs: HashMap<String, usize>,
    /// Each accordion's open sections, and how many times each was toggled (its animation).
    pub accordion: HashMap<String, HashSet<usize>>,
    pub toggles: HashMap<String, u32>,
    pub carousel: HashMap<String, Slide>,
    /// Field keys with the message of the rule they break.
    pub errors: HashMap<String, String>,
    /// The select whose options are showing.
    pub select_open: Option<String>,
    /// Callouts with their dismissal on its way.
    pub toasts: HashSet<String>,
    /// Modals with a fixed `open` that were closed.
    pub closed: HashSet<String>,
    /// The code block just copied.
    pub copied: Option<String>,
}

/// A field found while preparing: its key and binding.
struct Site {
    key: String,
    binding: Binding,
}

impl EventEmitter<CardEvent> for CardView {}

impl CardView {
    /// A card from an agent's program text. One that can't be read draws "This card couldn't
    /// be shown." ([`CardView::error`] says why), so a host that would rather show the
    /// message's text checks [`openui::parse`] first and uses [`CardView::from_program`].
    ///
    /// `id` prefixes every element id and debug selector the card makes, so several cards in
    /// one window don't collide: give each its own (a message id).
    pub fn new(
        source: &str,
        id: impl Into<SharedString>,
        host: Rc<dyn CardHost>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CardView {
        match openui::parse(source) {
            Ok(p) => CardView::from_program(p, id, host, window, cx),
            Err(e) => CardView::empty(id.into(), host, Err(e)),
        }
    }

    /// A card from a program already read.
    pub fn from_program(
        program: Program,
        id: impl Into<SharedString>,
        host: Rc<dyn CardHost>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CardView {
        let store = program.store();
        let card = program.render(&store);
        let mut v = CardView::empty(id.into(), host, card);
        v.store = store;
        v.program = Some(program);
        v.hydrate();
        v.prepare(window, cx);
        v
    }

    fn empty(id: SharedString, host: Rc<dyn CardHost>, card: Result<Card, CardError>) -> CardView {
        CardView {
            id,
            host,
            program: None,
            store: Store::default(),
            card,
            limits: Limits::default(),
            images: HashMap::new(),
            ui: Ui::default(),
            texts: HashMap::new(),
            sliders: HashMap::new(),
            dates: HashMap::new(),
            edits: HashMap::new(),
        }
    }

    /// Holds the card to other limits than [`Limits::default`].
    pub fn with_limits(mut self, limits: Limits) -> CardView {
        self.limits = limits;
        self
    }

    /// Starts from state the host kept ([`CardView::store`] from before), as when a chat list
    /// draws a message again.
    pub fn with_store(
        mut self,
        store: Store,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CardView {
        self.store = store;
        self.reread();
        self.sync_fields(window, cx);
        self
    }

    /// Why the card can't be shown, if it can't.
    pub fn error(&self) -> Option<&CardError> {
        self.card.as_ref().err()
    }

    /// The card as it stands.
    pub fn card(&self) -> Option<&Card> {
        self.card.as_ref().ok()
    }

    /// The card's state: its $variables and form fields.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The prefix of the card's element ids.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Draws the card again: after the host's theme changed.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Opens every accordion section, as for printing the card or a picture of it.
    pub fn expand_all(&mut self, cx: &mut Context<Self>) {
        fn sections(c: &Component, path: &str, out: &mut Vec<(String, usize)>) {
            if let Component::Accordion(s) = c {
                out.push((path.to_string(), s.len()));
            }
            for (seg, kid) in draw::kids(c) {
                sections(kid, &format!("{path}.{seg}"), out);
            }
        }
        let mut found = Vec::new();
        if let Ok(card) = &self.card {
            sections(&card.root, "r", &mut found);
        }
        for (path, n) in found {
            self.ui.accordion.insert(path, (0..n).collect());
        }
        cx.notify();
    }

    /// Presses a button (or anything pressable) by what it says, as the user would: for a
    /// host's own shortcuts and for tests. False when there's no such button.
    pub fn press_label(
        &mut self,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(card) = &self.card else { return false };
        let Some((action, form, check)) = draw::find_button(card, label) else {
            return false;
        };
        self.press(&action, label, form.as_deref(), check, window, cx);
        true
    }

    pub(crate) fn image_arrived(
        &mut self,
        src: ImageSrc,
        image: Option<Arc<Image>>,
        cx: &mut Context<Self>,
    ) {
        self.images
            .insert(src, image.map_or(ImageState::Refused, ImageState::Ready));
        cx.notify();
    }

    /// Reads the card again from the store.
    fn reread(&mut self) {
        if let Some(p) = &self.program {
            self.card = p.render(&self.store);
        }
    }

    /// After the store changed: read the card again, tell the host, draw.
    pub(crate) fn changed(&mut self, cx: &mut Context<Self>) {
        self.reread();
        cx.emit(CardEvent::Changed);
        let (host, store) = (self.host.clone(), self.store.clone());
        cx.defer(move |cx| host.changed(&store, cx));
        cx.notify();
    }

    /// Chips and option cards write their defaults into the form as soon as they're drawn, as
    /// react-ui's do, so a press carries them.
    fn hydrate(&mut self) {
        let mut sites = Vec::new();
        if let Ok(card) = &self.card {
            walk(
                &card.root,
                "r",
                None,
                &mut sites,
                &mut Vec::new(),
                &mut Vec::new(),
            );
        }
        for s in sites {
            if self.read(&s.binding).is_none()
                && let Some(v) = s.binding.field.hydrated()
            {
                self.write(&s.binding, v);
            }
        }
        self.reread();
    }

    /// A field's value: its $variable, else its entry in its form, else what was written in
    /// its place.
    pub(crate) fn read(&self, b: &Binding) -> Option<Json> {
        match &b.field.value {
            Some(Bound::Var(name)) => self.store.var(name).filter(|v| !v.is_null()),
            Some(Bound::Fixed(v)) => self
                .store
                .field(b.form.as_deref(), &b.field.name)
                .or_else(|| Some(v.clone())),
            None => self.store.field(b.form.as_deref(), &b.field.name),
        }
    }

    pub(crate) fn write(&mut self, b: &Binding, value: Json) {
        match &b.field.value {
            Some(Bound::Var(name)) => self.store.set_var(name, value),
            _ => self
                .store
                .set_field(b.form.as_deref(), b.field.kind(), &b.field.name, value),
        }
    }

    /// A field changed by the user: written, its error cleared, the card read again.
    pub(crate) fn edit(&mut self, key: &str, b: &Binding, value: Json, cx: &mut Context<Self>) {
        self.write(b, value);
        self.ui.errors.remove(key);
        self.changed(cx);
    }

    /// Checks a field's rules, keeping the message of the first it breaks.
    pub(crate) fn validate(&mut self, key: &str, b: &Binding) -> bool {
        let value = self.read(b).unwrap_or(Json::Null);
        match b.field.rules.check(&value) {
            Some(msg) => {
                self.ui.errors.insert(key.to_string(), msg);
                false
            }
            None => {
                self.ui.errors.remove(key);
                true
            }
        }
    }

    /// Runs a press. `check` is the form's fields when the press checks them first (a primary
    /// button in a form): one that breaks a rule stops it, with the message under the field.
    pub(crate) fn press(
        &mut self,
        action: &Action,
        words: &str,
        form: Option<&str>,
        check: Option<Vec<(String, Binding)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.program.is_none() {
            return;
        }
        if let Some(fields) = check.filter(|_| action.checks_form()) {
            let mut ok = true;
            for (key, b) in &fields {
                ok &= self.validate(key, b);
            }
            if !ok {
                cx.notify();
                return;
            }
        }
        let Some(program) = &self.program else { return };
        let effects = program.act(action, words, form, &mut self.store);
        self.changed(cx);
        self.sync_fields(window, cx);
        self.deliver(effects, cx);
    }

    /// Hands what a press left to the host (deferred, so the host may update this card).
    pub(crate) fn deliver(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        for e in effects {
            let host = self.host.clone();
            match e {
                Effect::Send(text) => {
                    cx.emit(CardEvent::Send(text.clone()));
                    cx.defer(move |cx| host.send(&text, cx));
                }
                Effect::Open(link) => {
                    cx.emit(CardEvent::Open(link.clone()));
                    cx.defer(move |cx| host.open_link(&link, cx));
                }
            }
        }
    }

    /// A link from Markdown, kept only when it's http or https.
    pub(crate) fn open_markdown_link(&mut self, url: &str, cx: &mut Context<Self>) {
        if let Some(link) = openui::Link::parse(url) {
            self.deliver(vec![Effect::Open(link)], cx);
        }
    }

    /// Puts the store's values into fields that show something else (after a press set or
    /// reset their $variables).
    pub(crate) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let want: Vec<(String, String)> = self
            .texts
            .iter()
            .map(|(k, e)| (k.clone(), text_of(self.read(&e.binding))))
            .collect();
        for (key, text) in want {
            let Some(e) = self.texts.get(&key) else {
                continue;
            };
            if e.state.read(cx).value().as_ref() != text {
                e.state.update(cx, |s, cx| s.set_value(text, window, cx));
            }
        }
        let want: Vec<(String, f32)> = self
            .sliders
            .iter()
            .map(|(k, e)| (k.clone(), slider_value(&e.binding, self.read(&e.binding))))
            .collect();
        for (key, v) in want {
            if let Some(e) = self.sliders.get(&key)
                && (e.state.read(cx).value() - v).abs() > f32::EPSILON
            {
                e.state.update(cx, |s, cx| s.set_value(v, cx));
            }
        }
        let want: Vec<(String, DateEnds)> = self
            .dates
            .iter()
            .map(|(k, e)| (k.clone(), date_ends(self.read(&e.binding))))
            .collect();
        for (key, (start, end)) in want {
            if let Some(e) = self.dates.get(&key) {
                let now = e.state.read(cx).value();
                if (now.start(), now.end()) != (start, end) {
                    e.state.update(cx, |s, cx| s.set_value(start, end, cx));
                }
            }
        }
    }

    /// Before drawing: an entity for every text field, slider and date picker the card has,
    /// and a request to the host for every image it hasn't asked about.
    pub(crate) fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (mut sites, mut srcs, mut tables) = (Vec::new(), Vec::new(), Vec::new());
        if let Ok(card) = &self.card {
            walk(&card.root, "r", None, &mut sites, &mut srcs, &mut tables);
        }
        for s in sites {
            self.ensure_field(s, window, cx);
        }
        for src in srcs {
            if self.images.contains_key(&src) {
                continue;
            }
            self.images.insert(src.clone(), ImageState::Pending);
            let reply = ImageReply {
                view: cx.entity().downgrade(),
                src: src.clone(),
            };
            let state = match self.host.image(&src, reply, cx) {
                ImageLoad::Ready(img) => ImageState::Ready(img),
                ImageLoad::Pending => ImageState::Pending,
                ImageLoad::Refused => ImageState::Refused,
            };
            self.images.insert(src, state);
        }
        for (key, rows) in tables {
            self.edits.entry(key).or_insert(EditTable {
                rows,
                editing: None,
                dirty: false,
            });
        }
        let mut toasts = Vec::new();
        if let Ok(card) = &self.card {
            find_toasts(&card.root, "r", &mut toasts);
        }
        for (path, var) in toasts {
            if self.store.var(&var).is_some_and(|v| truthy(&v)) {
                self.schedule_toast(&path, &var, cx);
            }
        }
    }

    fn ensure_field(&mut self, s: Site, window: &mut Window, cx: &mut Context<Self>) {
        let Site { key, binding } = s;
        match &binding.field.input {
            Input::Text { kind, placeholder } => {
                if let Some(e) = self.texts.get_mut(&key) {
                    e.binding = binding;
                    return;
                }
                let value = text_of(self.read(&binding));
                let (kind, placeholder) = (*kind, placeholder.clone().unwrap_or_default());
                let state = cx.new(|cx| {
                    let s = InputState::new(window, cx)
                        .placeholder(placeholder)
                        .masked(kind == InputKind::Password)
                        .default_value(value);
                    match keyboard(kind) {
                        Some(k) => s.keyboard(k),
                        None => s,
                    }
                });
                let subs = self.text_subscriptions(&key, &state, window, cx);
                self.texts.insert(
                    key,
                    TextEntry {
                        state,
                        binding,
                        _subs: subs,
                    },
                );
            }
            Input::TextArea { placeholder, rows } => {
                if let Some(e) = self.texts.get_mut(&key) {
                    e.binding = binding;
                    return;
                }
                let value = text_of(self.read(&binding));
                let rows = rows.unwrap_or(3.).clamp(1., 12.) as usize;
                let placeholder = placeholder.clone().unwrap_or_default();
                // As tall as its rows, and then as its text, as the web's (field-sizing:
                // content) does, up to the most rows a card asks for.
                let state = cx.new(|cx| {
                    InputState::textarea(window, cx)
                        .auto_grow(rows, rows.max(12))
                        .placeholder(placeholder)
                        .default_value(value)
                });
                let subs = self.text_subscriptions(&key, &state, window, cx);
                self.texts.insert(
                    key,
                    TextEntry {
                        state,
                        binding,
                        _subs: subs,
                    },
                );
            }
            Input::Slider { min, max, step, .. } => {
                if let Some(e) = self.sliders.get_mut(&key) {
                    e.binding = binding;
                    return;
                }
                let (lo, hi) = slider_range(*min, *max);
                let step = step
                    .filter(|s| s.is_finite() && *s > 0.)
                    .map_or((hi - lo) / 100., |s| s as f32);
                let value = slider_value(&binding, self.read(&binding));
                let state = cx.new(|_| {
                    SliderState::new()
                        .min(lo)
                        .max(hi)
                        .step(step.max(f32::EPSILON))
                        .default_value(value)
                });
                let k = key.clone();
                let sub = cx.subscribe(
                    &state,
                    move |this: &mut CardView, _, e: &SliderEvent, cx| {
                        let (SliderEvent::Release(v) | SliderEvent::Change(v)) = e;
                        if let Some(b) = this.sliders.get(&k).map(|e| e.binding.clone()) {
                            let n = round_to(*v as f64);
                            if this.read(&b).as_ref() != Some(&json!([n])) {
                                this.edit(&k, &b, json!([n]), cx);
                            }
                        }
                    },
                );
                self.sliders.insert(
                    key,
                    SliderEntry {
                        state,
                        binding,
                        _sub: sub,
                    },
                );
            }
            Input::Date { range } => {
                if let Some(e) = self.dates.get_mut(&key) {
                    e.binding = binding;
                    return;
                }
                let range = *range;
                // Showing what the store already has (a card restored with its store, or a
                // value the program wrote), without telling anyone.
                let (start, end) = date_ends(self.read(&binding));
                let state = cx.new(|cx| {
                    let mut s = if range {
                        DatePickerState::range(window, cx)
                    } else {
                        DatePickerState::new(window, cx)
                    };
                    if start.is_some() {
                        s.set_value(start, end, cx);
                    }
                    s
                });
                let k = key.clone();
                let sub = cx.subscribe(
                    &state,
                    move |this: &mut CardView, _, e: &DatePickerEvent, cx| {
                        let DatePickerEvent::Change(dt) = e;
                        let Some(b) = this.dates.get(&k).map(|e| e.binding.clone()) else {
                            return;
                        };
                        let day = |d: Option<chrono::NaiveDate>| {
                            d.map(|d| d.format("%Y-%m-%d").to_string())
                        };
                        let value = if range {
                            match (day(dt.start()), day(dt.end())) {
                                (None, None) => Json::Null,
                                (from, to) => json!({ "from": from, "to": to }),
                            }
                        } else {
                            day(dt.start()).map_or(Json::Null, Json::String)
                        };
                        this.edit(&k, &b, value, cx);
                    },
                );
                self.dates.insert(
                    key,
                    DateEntry {
                        state,
                        binding,
                        _sub: sub,
                    },
                );
            }
            _ => {}
        }
    }

    fn text_subscriptions(
        &mut self,
        key: &str,
        state: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let k = key.to_string();
        vec![cx.subscribe_in(
            state,
            window,
            move |this: &mut CardView, state, e: &InputEvent, _, cx| {
                let Some(b) = this.texts.get(&k).map(|e| e.binding.clone()) else {
                    return;
                };
                match e {
                    InputEvent::Change => {
                        let text = state.read(cx).value().to_string();
                        if text_of(this.read(&b)) != text {
                            this.edit(&k, &b, Json::String(text), cx);
                        }
                    }
                    // As the web's inputs: checked when left, cleared when entered. Either way
                    // the card draws again, for the field's focus ring.
                    InputEvent::Blur => {
                        if !b.field.rules.is_empty() {
                            this.validate(&k, &b);
                        }
                        cx.notify();
                    }
                    InputEvent::Focus => {
                        this.ui.errors.remove(&k);
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => {}
                }
            },
        )]
    }

    /// Starts editing an editable table's cell: an input with the cell's text.
    pub(crate) fn edit_cell(
        &mut self,
        key: &str,
        row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.edits.get(key) else { return };
        let text = t
            .rows
            .get(row)
            .and_then(|r| r.get(col))
            .cloned()
            .unwrap_or_default();
        let state = cx.new(|cx| InputState::new(window, cx).default_value(text));
        let k = key.to_string();
        let sub = cx.subscribe_in(
            &state,
            window,
            move |this: &mut CardView, state, e: &InputEvent, _, cx| match e {
                InputEvent::Change => {
                    let text = state.read(cx).value().to_string();
                    if let Some(t) = this.edits.get_mut(&k)
                        && let Some(cell) = t.rows.get_mut(row).and_then(|r| r.get_mut(col))
                    {
                        *cell = text;
                        t.dirty = true;
                    }
                }
                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                    if let Some(t) = this.edits.get_mut(&k) {
                        t.editing = None;
                    }
                    cx.notify();
                }
                InputEvent::Focus => {}
            },
        );
        state.update(cx, |s, cx| s.focus(window, cx));
        if let Some(t) = self.edits.get_mut(key) {
            t.editing = Some((row, col, state, sub));
        }
        cx.notify();
    }

    /// Sets an editable table's cell to a value (a select column's choice).
    pub(crate) fn set_cell(
        &mut self,
        key: &str,
        row: usize,
        col: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(t) = self.edits.get_mut(key)
            && let Some(cell) = t.rows.get_mut(row).and_then(|r| r.get_mut(col))
        {
            *cell = value;
            t.dirty = true;
        }
        self.ui.select_open = None;
        cx.notify();
    }

    /// The web's Save Changes: the rows into the form as `[{ id, values }]`, then a press of
    /// "Save Changes" in that form.
    pub(crate) fn save_table(
        &mut self,
        key: &str,
        table: &openui::EditableTable,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.edits.get_mut(key) else {
            return;
        };
        t.editing = None;
        t.dirty = false;
        let rows: Vec<Json> = t
            .rows
            .iter()
            .enumerate()
            .map(|(r, values)| {
                let values: Vec<Json> = values
                    .iter()
                    .enumerate()
                    .map(|(c, v)| match table.columns.get(c).map(|c| c.kind.as_str()) {
                        Some("number") => v.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map_or_else(|| json!(v), |n| json!(n)),
                        _ => json!(v),
                    })
                    .collect();
                json!({ "id": table.rows.get(r).map(|r| r.id.clone()).unwrap_or_default(), "values": values })
            })
            .collect();
        self.store.set_field(
            Some(&table.name),
            Some("EditableTable"),
            &table.name,
            Json::Array(rows),
        );
        self.press(
            &Action::default_words(),
            "Save Changes",
            Some(&table.name),
            None,
            window,
            cx,
        );
    }

    /// A callout bound to a $variable hides itself three seconds after it shows, by setting
    /// the variable false (react-ui's toast). One timer, once; nothing runs while it waits.
    pub(crate) fn schedule_toast(&mut self, path: &str, var: &str, cx: &mut Context<Self>) {
        if !self.ui.toasts.insert(path.to_string()) {
            return;
        }
        let (path, var) = (path.to_string(), var.to_string());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                this.ui.toasts.remove(&path);
                if this.store.var(&var).is_some_and(|v| truthy(&v)) {
                    this.store.set_var(&var, Json::Bool(false));
                    this.changed(cx);
                }
            });
        })
        .detach();
    }

    /// Closes a modal: false into its $variable, or closed for good when its `open` was fixed.
    pub(crate) fn close_modal(&mut self, path: &str, open: &Bound, cx: &mut Context<Self>) {
        match open {
            Bound::Var(name) => {
                self.store.set_var(name, Json::Bool(false));
                self.changed(cx);
            }
            Bound::Fixed(_) => {
                self.ui.closed.insert(path.to_string());
                cx.notify();
            }
        }
    }

    /// Whether a modal is open.
    pub(crate) fn modal_open(&self, path: &str, open: &Bound) -> bool {
        match open {
            Bound::Var(name) => self.store.var(name).is_some_and(|v| truthy(&v)),
            Bound::Fixed(v) => truthy(v) && !self.ui.closed.contains(path),
        }
    }

    /// Copies a code block's text (the one thing the card writes anywhere: the clipboard, when
    /// the user presses Copy).
    pub(crate) fn copy(&mut self, path: &str, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()));
        self.ui.copied = Some(path.to_string());
        cx.notify();
    }
}

/// The text a field's value shows: a string as it is, a number as JavaScript writes it.
pub(crate) fn text_of(v: Option<Json>) -> String {
    match v {
        Some(Json::String(s)) => s,
        Some(Json::Number(n)) => {
            let f = n.as_f64().unwrap_or(0.);
            if f.fract() == 0. && f.abs() < 1e15 {
                format!("{}", f as i64)
            } else {
                n.to_string()
            }
        }
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// JavaScript's truthiness of a JSON value.
pub(crate) fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64().is_some_and(|f| f != 0. && !f.is_nan()),
        Json::String(s) => !s.is_empty() && s != "false",
        _ => true,
    }
}

/// A slider's range, never empty or backwards.
pub(crate) fn slider_range(min: f64, max: f64) -> (f32, f32) {
    let lo = if min.is_finite() { min as f32 } else { 0. };
    let hi = if max.is_finite() { max as f32 } else { 100. };
    if hi > lo { (lo, hi) } else { (lo, lo + 1.) }
}

/// A date picker's two ends (the second only in a range).
type DateEnds = (Option<chrono::NaiveDate>, Option<chrono::NaiveDate>);

/// What a date field's stored value says the picker shows: "YYYY-MM-DD", or a range's
/// `{from, to}` of them. Anything else is no date.
fn date_ends(v: Option<Json>) -> DateEnds {
    let day = |v: Option<&Json>| {
        v.and_then(Json::as_str)
            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    };
    match &v {
        Some(Json::String(_)) => (day(v.as_ref()), None),
        // A range's end only with its start, as the picker keeps them.
        Some(Json::Object(o)) => match day(o.get("from")) {
            Some(from) => (Some(from), day(o.get("to"))),
            None => (None, None),
        },
        _ => (None, None),
    }
}

fn slider_value(b: &Binding, v: Option<Json>) -> f32 {
    let Input::Slider {
        min, max, default, ..
    } = &b.field.input
    else {
        return 0.;
    };
    let (lo, hi) = slider_range(*min, *max);
    let n = match v {
        Some(Json::Array(a)) => a.first().and_then(Json::as_f64),
        Some(Json::Number(n)) => n.as_f64(),
        _ => None,
    }
    .or_else(|| default.first().copied())
    .unwrap_or(lo as f64);
    (n as f32).clamp(lo, hi)
}

fn round_to(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// The on-screen keyboard for an address, a link or a number, as the web's input types
/// give one: no capitals, corrections or suggestions. Text and passwords keep gpui-core's own.
fn keyboard(kind: InputKind) -> Option<TextInputConfiguration> {
    matches!(kind, InputKind::Email | InputKind::Url | InputKind::Number).then_some(
        TextInputConfiguration {
            autocorrect: false,
            autocapitalize: Autocapitalize::None,
            suggestions: false,
            input_action: TextInputAction::Done,
        },
    )
}

/// Finds the fields, images and editable tables under a component, with the keys the drawing
/// gives them (draw::kids names the paths for both).
fn walk(
    c: &Component,
    path: &str,
    form: Option<&str>,
    sites: &mut Vec<Site>,
    srcs: &mut Vec<ImageSrc>,
    tables: &mut Vec<(String, Vec<Vec<String>>)>,
) {
    match c {
        Component::Form(f) => {
            for (i, fc) in f.fields.iter().enumerate() {
                sites.push(Site {
                    key: draw::field_key(path, i),
                    binding: Binding {
                        form: Some(f.name.clone()),
                        field: fc.field.clone(),
                    },
                });
                collect_choice_images(&fc.field, srcs);
            }
        }
        Component::Field(field) => {
            sites.push(Site {
                key: path.to_string(),
                binding: Binding {
                    form: form.map(str::to_string),
                    field: field.clone(),
                },
            });
            collect_choice_images(field, srcs);
        }
        Component::EditableTable(t) => tables.push((
            path.to_string(),
            t.rows
                .iter()
                .map(|r| {
                    (0..t.columns.len())
                        .map(|i| r.values.get(i).cloned().unwrap_or_default())
                        .collect()
                })
                .collect(),
        )),
        Component::Image(i) => srcs.extend(i.src.clone()),
        Component::Gallery(g) => srcs.extend(g.iter().filter_map(|i| i.src.clone())),
        Component::ImageText(i) => srcs.extend(i.src.clone()),
        Component::List(l) => srcs.extend(
            l.items
                .iter()
                .filter_map(|i| i.image.as_ref().and_then(|i| i.src.clone())),
        ),
        Component::Cards(b) => srcs.extend(b.items.iter().filter_map(|i| i.background.clone())),
        _ => {}
    }
    for (seg, kid) in draw::kids(c) {
        walk(kid, &format!("{path}.{seg}"), form, sites, srcs, tables);
    }
}

/// Callouts bound to a $variable, by path.
fn find_toasts(c: &Component, path: &str, out: &mut Vec<(String, String)>) {
    if let Component::Callout(openui::Callout {
        visible: Some(Bound::Var(var)),
        ..
    }) = c
    {
        out.push((path.to_string(), var.clone()));
    }
    for (seg, kid) in draw::kids(c) {
        find_toasts(kid, &format!("{path}.{seg}"), out);
    }
}

fn collect_choice_images(field: &Field, srcs: &mut Vec<ImageSrc>) {
    if let Input::OptionCards { options, .. }
    | Input::Radio { options, .. }
    | Input::Chips { options, .. }
    | Input::Select { options, .. } = &field.input
    {
        srcs.extend(
            options
                .iter()
                .filter_map(|o| o.image.as_ref().and_then(|i| i.src.clone())),
        );
    }
}
