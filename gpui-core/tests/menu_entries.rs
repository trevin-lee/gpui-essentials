//! A menu's headings (shown, passed over), an entry drawn by the host (a pill) under its text
//! label, the host's row height and text size, and a picker's Create entry. (The check's colour
//! is drawn by an icon, and the test platform has no assets to draw icons with.)

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, ParentElement as _, Render, SharedString, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};
use gpui_core::{MenuSelect, Picker, PopupMenu, PopupMenuColors, PopupMenuItem, Root};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

type Log = Rc<RefCell<Vec<String>>>;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Menu,
    MenuTouch,
    /// 40px text and no row height of its own.
    MenuBigText,
    Picker,
}

struct Panel {
    kind: Kind,
    log: Log,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        let pick = move |p: &MenuSelect, _: &mut Window, _: &mut gpui::App| {
            log.borrow_mut().push(p.id.to_string())
        };
        let body = match self.kind {
            Kind::Menu | Kind::MenuTouch => PopupMenu::new("menu")
                .touch(self.kind == Kind::MenuTouch)
                .row_height(px(28.))
                .text_size(px(13.))
                .colors(PopupMenuColors {
                    check: Some(RED),
                    ..Default::default()
                })
                .heading("Status")
                .item(
                    PopupMenuItem::new("done", "Done")
                        .checked(true)
                        .label_element(
                            div()
                                .debug_selector(|| "pill".into())
                                .px(px(6.))
                                .rounded_full()
                                .bg(RED)
                                .child("Done"),
                        ),
                )
                .item(PopupMenuItem::new("todo", "To do"))
                .heading("Other")
                .item(PopupMenuItem::new("later", "Later"))
                .on_select(pick)
                .into_any_element(),
            Kind::MenuBigText => PopupMenu::new("menu")
                .text_size(px(40.))
                .item(PopupMenuItem::new("todo", "To do"))
                .on_select(pick)
                .into_any_element(),
            Kind::Picker => {
                let log = self.log.clone();
                Picker::new("picker")
                    .item(PopupMenuItem::new("work", "Work"))
                    .item(PopupMenuItem::new("home", "Home"))
                    .on_select(pick)
                    .on_create(move |text, _, _| log.borrow_mut().push(format!("create:{text}")))
                    .into_any_element()
            }
        };
        let _ = cx;
        div().size_full().p(px(20.)).w(px(260.)).child(body)
    }
}

fn open(cx: &mut TestAppContext, kind: Kind) -> (Entity<Panel>, Log, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let log: Log = Rc::default();
    let l = log.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel { kind, log: l });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    (panel, log, vcx)
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
fn headings_are_passed_over(cx: &mut TestAppContext) {
    let (_, log, vcx) = open(cx, Kind::Menu);
    // It opens on Done, below the first heading; down twice passes "Other" to Later.
    keys(vcx, "enter down down enter");
    assert_eq!(*log.borrow(), ["done", "later"]);
}

#[gpui::test]
fn the_host_draws_an_entrys_label(cx: &mut TestAppContext) {
    let (_, _, vcx) = open(cx, Kind::Menu);
    let pill = vcx.debug_bounds("pill").expect("the pill is drawn");
    let row = vcx.debug_bounds("menu-item-done").unwrap();
    assert!(row.contains(&pill.center()), "{pill:?} in {row:?}");
}

#[gpui::test]
fn rows_take_the_hosts_height_and_touch_takes_44_points(cx: &mut TestAppContext) {
    let (_, _, vcx) = open(cx, Kind::Menu);
    let row = vcx.debug_bounds("menu-item-todo").unwrap();
    assert_eq!(row.size.height, px(28.));
    let mut cx2 = TestAppContext::single();
    let (_, _, vcx) = open(&mut cx2, Kind::MenuTouch);
    let row = vcx.debug_bounds("menu-item-todo").unwrap();
    assert!(row.size.height >= px(44.), "{row:?}");
}

#[gpui::test]
fn a_picker_offers_to_create_what_isnt_there(cx: &mut TestAppContext) {
    let (_, log, vcx) = open(cx, Kind::Picker);
    vcx.simulate_input("  Errands ");
    // Nothing matches, so Create is the only entry, and Enter picks it.
    let create: SharedString = "menu-item-\u{0}gpui-core-picker-create".into();
    let _ = create;
    keys(vcx, "enter");
    assert_eq!(*log.borrow(), ["create:Errands"]);
}

#[gpui::test]
fn a_picker_offers_no_create_for_an_entry_it_has(cx: &mut TestAppContext) {
    let (_, log, vcx) = open(cx, Kind::Picker);
    vcx.simulate_input("WORK");
    keys(vcx, "down enter");
    // Down from Work wraps to Work: there's no Create entry after it.
    assert_eq!(*log.borrow(), ["work"]);
}

#[gpui::test]
fn entries_take_the_hosts_text_size(cx: &mut TestAppContext) {
    let (_, _, vcx) = open(cx, Kind::MenuBigText);
    let row = vcx.debug_bounds("menu-item-todo").unwrap();
    assert!(
        row.size.height > px(40.),
        "the row grows to its 40px text: {row:?}"
    );
}
