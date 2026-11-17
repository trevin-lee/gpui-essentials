//! Combobox: a press or the keys (Enter, Space, Down, Up) open it with the keyboard in its search
//! field and the chosen option highlighted, a second press closes it, typing filters the options
//! and shows a note when nothing matches, Up, Down and Enter choose, choosing tells the host,
//! closes it and gives the keyboard back to the trigger (as Escape does), the host keeps the
//! value, long labels are cut short in the trigger and the list (as wide as the trigger), a
//! disabled one doesn't open, touch makes it 44 points, and nothing redraws while idle. Laid out
//! with Core Text.

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ParentElement as _, Pixels, Render, SharedString, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Combobox, Disableable as _, Press, Root, Sizable as _};

const LONG: &str = "A framework whose name goes on and on far past the width of any trigger";

struct Host {
    value: Option<SharedString>,
    disabled: bool,
    touch: bool,
    changes: Rc<RefCell<Vec<String>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let changes = self.changes.clone();
        let combobox = Combobox::new("framework")
            .placeholder("Select framework...")
            .search_placeholder("Search framework...")
            .empty_text("No framework found.")
            .options([
                ("next", "Next.js"),
                ("svelte", "SvelteKit"),
                ("nuxt", "Nuxt.js"),
                ("remix", "Remix"),
                ("long", LONG),
            ])
            .disable("nuxt")
            .accessibility_label("Framework")
            .selected(self.value.clone())
            .disabled(self.disabled)
            .on_change(cx.listener(move |this, id: &SharedString, _, cx| {
                changes.borrow_mut().push(id.to_string());
                this.value = Some(id.clone());
                cx.notify();
            }));
        div()
            .size_full()
            .p(px(20.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(if self.touch {
                combobox.touch()
            } else {
                combobox
            })
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    vcx: &'a mut VisualTestContext,
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|cx| Host {
            value: None,
            disabled: false,
            touch: false,
            changes: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    Opened { host, vcx }
}

impl Opened<'_> {
    fn bounds(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.vcx.debug_bounds(selector)
    }

    fn is_open(&mut self) -> bool {
        self.bounds("gpui-core-popover").is_some()
    }

    fn click(&mut self, selector: &'static str) {
        let at = self.bounds(selector).expect(selector).center();
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn keys(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
            self.vcx.run_until_parked();
        }
    }

    fn type_text(&mut self, text: &str) {
        self.vcx.simulate_input(text);
        self.vcx.run_until_parked();
    }

    /// The options shown, by id.
    fn shown(&mut self) -> Vec<&'static str> {
        [
            ("next", "menu-item-next"),
            ("svelte", "menu-item-svelte"),
            ("nuxt", "menu-item-nuxt"),
            ("remix", "menu-item-remix"),
            ("long", "menu-item-long"),
        ]
        .into_iter()
        .filter(|(_, sel)| self.vcx.debug_bounds(sel).is_some())
        .map(|(id, _)| id)
        .collect()
    }

    fn changes(&mut self) -> Vec<String> {
        self.host
            .read_with(self.vcx, |h, _| h.changes.borrow().clone())
    }

    fn set(&mut self, f: impl FnOnce(&mut Host)) {
        self.host.update(self.vcx, |h, cx| {
            f(h);
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    /// Tab from the Press before it to the trigger.
    fn tab_in(&mut self) {
        let before = self.host.read_with(self.vcx, |h, _| h.before.clone());
        self.vcx.update(|window, cx| window.focus(&before, cx));
        self.keys("tab");
    }

    /// Whether the trigger has the keyboard: Enter on it opens the list.
    fn trigger_has_the_keyboard(&mut self) -> bool {
        assert!(!self.is_open());
        self.keys("enter");
        let open = self.is_open();
        if open {
            self.keys("escape");
        }
        open
    }

    fn draws(&mut self) -> usize {
        self.host.read_with(self.vcx, |h, _| h.draws.get())
    }
}

#[test]
fn a_press_opens_it_and_a_press_on_an_option_chooses_it() {
    let mut cx = app();
    let mut o = open(&mut cx);
    assert!(!o.is_open());
    o.click("combobox-framework");
    assert!(o.is_open());
    assert_eq!(o.shown(), ["next", "svelte", "nuxt", "remix", "long"]);
    o.click("menu-item-remix");
    assert_eq!(o.changes(), ["remix"]);
    assert!(!o.is_open(), "choosing closes it");
    assert!(o.trigger_has_the_keyboard(), "and gives the keyboard back");
    o.click("combobox-framework");
    o.click("combobox-framework");
    assert!(!o.is_open(), "a second press on the trigger closes it");
}

#[test]
fn the_keys_open_it_and_choose() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.tab_in();
    for key in ["enter", "space", "down", "up"] {
        o.keys(key);
        assert!(o.is_open(), "{key} opens it");
        o.keys("escape");
        assert!(!o.is_open(), "Escape closes it");
    }
    // Next.js highlighted; Down passes over Nuxt.js, which is disabled.
    o.keys("down");
    o.keys("down down enter");
    assert_eq!(o.changes(), ["remix"]);
    assert!(!o.is_open());
    assert!(o.trigger_has_the_keyboard());
}

#[test]
fn it_opens_on_the_chosen_option() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.set(|h| h.value = Some("svelte".into()));
    o.tab_in();
    o.keys("down enter");
    assert_eq!(o.changes(), ["svelte"], "the chosen option was highlighted");
    o.keys("down down enter");
    assert_eq!(o.changes(), ["svelte", "remix"]);
}

#[test]
fn typing_filters_and_nothing_matching_shows_a_note() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.click("combobox-framework");
    assert!(o.bounds("picker-empty").is_none());
    o.type_text("js");
    assert_eq!(o.shown(), ["next", "nuxt"]);
    assert!(o.bounds("picker-empty").is_none());
    o.type_text("zz");
    assert!(o.shown().is_empty());
    let note = o.bounds("picker-empty").expect("the note");
    assert!(note.size.height > px(10.), "{note:?}");
    o.keys("enter");
    assert!(o.changes().is_empty(), "nothing to choose");
    assert!(o.is_open());
    o.keys("backspace backspace backspace backspace");
    o.type_text("REM");
    assert_eq!(o.shown(), ["remix"]);
    o.keys("enter");
    assert_eq!(o.changes(), ["remix"]);
    // Opened again, the search starts empty.
    o.click("combobox-framework");
    assert_eq!(o.shown().len(), 5);
}

#[test]
fn escape_closes_it_and_gives_the_keyboard_back() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.click("combobox-framework");
    o.type_text("sv");
    o.keys("escape");
    assert!(!o.is_open());
    assert!(o.changes().is_empty());
    assert!(o.trigger_has_the_keyboard());
}

#[test]
fn the_host_keeps_the_value() {
    let mut cx = app();
    let mut o = open(&mut cx);
    let text = |o: &mut Opened| o.bounds("combobox-framework-text").unwrap();
    let placeholder = text(&mut o);
    o.set(|h| h.value = Some("next".into()));
    let next = text(&mut o);
    assert!(next.size.width < placeholder.size.width, "shows Next.js");
    // A host that ignores the change keeps its value.
    o.set(|h| h.value = None);
    assert_eq!(text(&mut o).size.width, placeholder.size.width);
}

#[test]
fn long_labels_are_cut_short_in_the_trigger_and_the_list() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.set(|h| h.value = Some("long".into()));
    let trigger = o.bounds("combobox-framework").unwrap();
    let text = o.bounds("combobox-framework-text").unwrap();
    assert!(trigger.size.width <= px(201.), "200 wide: {trigger:?}");
    assert!(text.right() <= trigger.right(), "{text:?} in {trigger:?}");
    o.click("combobox-framework");
    let panel = o.bounds("gpui-core-popover").unwrap();
    assert!(
        (panel.size.width - trigger.size.width).abs() <= px(1.),
        "the list is the trigger's width: {panel:?} under {trigger:?}"
    );
    let label = o.bounds("menu-label-long").unwrap();
    assert!(label.right() <= panel.right(), "{label:?} in {panel:?}");
}

#[test]
fn a_disabled_one_doesnt_open() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.set(|h| h.disabled = true);
    o.click("combobox-framework");
    assert!(!o.is_open());
    o.tab_in();
    o.keys("down enter space");
    assert!(!o.is_open());
}

#[test]
fn a_touch_one_is_44_points_tall_and_so_are_its_options() {
    let mut cx = app();
    let mut o = open(&mut cx);
    o.set(|h| h.touch = true);
    let b = o.bounds("combobox-framework").unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
    o.click("combobox-framework");
    let row = o.bounds("menu-item-next").unwrap();
    assert!(row.size.height >= px(44.), "{row:?}");
}

#[test]
fn nothing_redraws_while_idle() {
    let mut cx = app();
    let mut o = open(&mut cx);
    let rest = |o: &mut Opened| {
        // Let a caret's blink settle first: the field's own, drawn in its own element.
        o.vcx.executor().advance_clock(Duration::from_secs(1));
        o.vcx.run_until_parked();
        let before = o.draws();
        for _ in 0..30 {
            o.vcx.update(|window, cx| window.simulate_next_frame(cx));
            o.vcx.executor().advance_clock(Duration::from_millis(16));
            o.vcx.run_until_parked();
        }
        assert_eq!(o.draws(), before);
    };
    rest(&mut o);
    o.click("combobox-framework");
    assert!(o.is_open());
    rest(&mut o);
}
