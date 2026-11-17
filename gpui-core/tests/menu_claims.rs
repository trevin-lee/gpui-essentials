//! The presses submenus claim (so their popover doesn't take a press on them for one outside it)
//! don't pile up: a host with a menu per row, each opened once with a submenu, keeps a bounded
//! number, and the one open still works.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ParentElement as _, Render, SharedString, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px, size,
};
use gpui_core::{MenuSelect, Popover, PopupMenu, PopupMenuItem, Press, Root};

struct Rows {
    open: bool,
    /// Which row's menu is open: each has its own ids.
    row: Rc<Cell<usize>>,
    picked: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row = self.row.get();
        let picked = self.picked.clone();
        div().size_full().p(px(20.)).child(
            Popover::new("pop")
                .open(self.open)
                .on_dismiss(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }))
                .trigger(
                    Press::new("trigger")
                        .debug_selector(|| "trigger".into())
                        .w(px(120.))
                        .h(px(30.))
                        .child("Row")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = !this.open;
                            cx.notify();
                        })),
                )
                .p(px(4.))
                .child(
                    PopupMenu::new(SharedString::from(format!("row-{row}")))
                        .item(PopupMenuItem::new("copy", "Copy"))
                        .item(
                            PopupMenuItem::new("move", "Move to").submenu(
                                PopupMenu::new(SharedString::from(format!("row-{row}-move")))
                                    .item(PopupMenuItem::new("inbox", "Inbox")),
                            ),
                        )
                        .on_select(cx.listener(move |this, pick: &MenuSelect, _, cx| {
                            picked.borrow_mut().push(pick.id.clone());
                            this.open = false;
                            cx.notify();
                        })),
                ),
        )
    }
}

fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
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

#[gpui::test]
fn claims_from_menus_long_gone_dont_pile_up(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let row = Rc::new(Cell::new(0));
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let (r, p) = (row.clone(), picked.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let v = cx.new(|_| Rows {
            open: false,
            row: r,
            picked: p,
        });
        Root::new(v, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let trigger = vcx.debug_bounds("trigger").unwrap().center();
    for n in 0..100 {
        row.set(n);
        vcx.simulate_click(trigger, Modifiers::none());
        vcx.run_until_parked();
        keys(vcx, "down right");
        assert!(vcx.debug_bounds("menu-submenu-move").is_some(), "row {n}");
        keys(vcx, "escape escape");
    }
    let claims = vcx.update(|_, cx| gpui_core::claim_count(cx));
    assert!(claims <= 32, "{claims} claims kept");
    // The one open now still keeps its press from closing the popover.
    vcx.simulate_click(trigger, Modifiers::none());
    vcx.run_until_parked();
    keys(vcx, "down right");
    let inbox = vcx.debug_bounds("menu-item-inbox").unwrap().center();
    vcx.simulate_click(inbox, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(*picked.borrow(), ["inbox"]);
}
