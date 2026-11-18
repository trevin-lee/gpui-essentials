//! CommandDialog with a host that searches: it's told what's typed as it changes (not when it
//! opens), and what it finds shows as given, past the filter, beside the commands that match,
//! and Enter picks it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    ParentElement as _, Render, SharedString, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px, size,
};
use gpui_core::{CommandDialog, MenuSelect, PopupMenuItem, Root};

struct Panel {
    /// What the host was told, in order.
    told: Rc<RefCell<Vec<String>>>,
    /// What it found for the last query.
    found: Vec<(SharedString, SharedString)>,
    picked: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let told = self.told.clone();
        let picked = self.picked.clone();
        div().size_full().child(
            CommandDialog::new("commands")
                .open(true)
                .group(
                    "Pages",
                    [
                        PopupMenuItem::new("agents", "Agents"),
                        PopupMenuItem::new("people", "People"),
                    ],
                )
                .found(
                    "Docs",
                    self.found
                        .iter()
                        .map(|(id, label)| PopupMenuItem::new(id.clone(), label.clone())),
                )
                .on_query_change(move |text, cx| {
                    told.borrow_mut().push(text.to_string());
                    // A search by meaning: "okafor" finds notes whose title doesn't say it.
                    let found = if text.trim() == "okafor" {
                        vec![("doc-1".into(), "Interview notes".into())]
                    } else {
                        Vec::new()
                    };
                    this.update(cx, |p, cx| {
                        p.found = found;
                        cx.notify();
                    })
                    .ok();
                })
                .on_select(move |pick: &MenuSelect, _, _| {
                    picked.borrow_mut().push(pick.id.clone())
                }),
        )
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Panel>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let p = cx.new(|_| Panel {
            told: Rc::default(),
            found: Vec::new(),
            picked: Rc::default(),
        });
        *m.borrow_mut() = Some(p.clone());
        Root::new(p, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let p = made.borrow().clone().unwrap();
    (p, vcx)
}

fn key(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

fn shown(vcx: &mut VisualTestContext, selector: &'static str) -> bool {
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn the_host_is_told_what_is_typed_and_its_finds_show_past_the_filter(cx: &mut TestAppContext) {
    let (p, vcx) = open(cx);
    let told = p.read_with(vcx, |p, _| p.told.clone());
    assert!(
        told.borrow().is_empty(),
        "not told on opening: {:?}",
        told.borrow()
    );
    assert!(shown(vcx, "menu-item-agents"));

    vcx.simulate_input("okafor");
    vcx.run_until_parked();
    assert_eq!(
        told.borrow().last().map(String::as_str),
        Some("okafor"),
        "{:?}",
        told.borrow()
    );
    // Each change told once.
    let n = told.borrow().len();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();
    assert_eq!(told.borrow().len(), n, "not told again on a redraw");

    // The pages don't match "okafor"; the doc found does, as given.
    assert!(!shown(vcx, "menu-item-agents"));
    assert!(
        shown(vcx, "menu-item-doc-1"),
        "the find shows though its title lacks the word"
    );
    key(vcx, "enter");
    let picked = p.read_with(vcx, |p, _| p.picked.clone());
    assert_eq!(picked.borrow().as_slice(), ["doc-1"]);

    // A query that finds nothing: the pages that match, and no finds.
    vcx.simulate_input(" x");
    vcx.run_until_parked();
    assert!(!shown(vcx, "menu-item-doc-1"));
}
