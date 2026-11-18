//! A select's long list: capped by `max_height`, it opens scrolled to the chosen option and
//! keeps the option the keys are on in view, going down past its bottom and back up past its
//! top. Wheeling it isn't undone while the highlight stays put.

use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, ParentElement as _, Pixels, Render, ScrollDelta, ScrollEvent, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, rems, size,
};
use gpui_core::{Root, Select};

const OPTIONS: usize = 60;

struct Host {
    chosen: SharedString,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut select = Select::new("zone")
            .accessibility_label("Time zone")
            .max_height(rems(10.))
            .selected(Some(self.chosen.clone()))
            .w(px(240.))
            .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                this.chosen = id.clone();
                cx.notify();
            }));
        for i in 0..OPTIONS {
            select = select.option(format!("z{i}"), format!("Zone {i}"));
        }
        div().p(px(20.)).child(select)
    }
}

fn open<'a>(cx: &'a mut TestAppContext, chosen: &str) -> (Entity<Host>, &'a mut VisualTestContext) {
    cx.update(gpui_core::init);
    let chosen = SharedString::from(chosen.to_string());
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|_| Host { chosen });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(800.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
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

fn draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();
}

fn bounds(vcx: &mut VisualTestContext, selector: &str) -> Bounds<Pixels> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't drawn"))
}

/// Whether option `i`'s row is wholly inside the open list.
fn shown(vcx: &mut VisualTestContext, i: usize) -> bool {
    let list = bounds(vcx, "gpui-core-popover");
    let row = bounds(vcx, &format!("menu-item-z{i}"));
    row.top() >= list.top() && row.bottom() <= list.bottom()
}

#[gpui::test]
fn it_opens_scrolled_to_the_chosen_option(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, "z45");
    let at = bounds(vcx, "select-zone").center();
    vcx.simulate_click(at, Modifiers::none());
    draw(vcx);
    let list = bounds(vcx, "gpui-core-popover");
    assert!(
        list.size.height <= px(10. * 16. + 2.),
        "capped at 10 rem: {list:?}"
    );
    assert!(shown(vcx, 45), "Zone 45, the chosen one, is in view");
    assert!(!shown(vcx, 0), "and Zone 0 is scrolled away");
}

#[gpui::test]
fn the_keys_keep_their_option_in_view(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, "z0");
    let at = bounds(vcx, "select-zone").center();
    vcx.simulate_click(at, Modifiers::none());
    draw(vcx);
    assert!(shown(vcx, 0));
    // Down past the list's bottom: each option is brought into view as the keys reach it.
    for i in 1..=20 {
        keys(vcx, "down");
        draw(vcx);
        assert!(shown(vcx, i), "Zone {i}, highlighted, is in view");
    }
    assert!(!shown(vcx, 0), "Zone 0 has scrolled away");
    // And back up past its top.
    for i in (0..20).rev() {
        keys(vcx, "up");
        draw(vcx);
        assert!(
            shown(vcx, i),
            "Zone {i}, highlighted, is in view on the way up"
        );
    }
    keys(vcx, "down down enter");
    assert_eq!(h.read_with(vcx, |h, _| h.chosen.to_string()), "z2");
}

#[gpui::test]
fn the_wheel_is_not_undone_while_the_highlight_stays(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, "z0");
    let at = bounds(vcx, "select-zone").center();
    vcx.simulate_click(at, Modifiers::none());
    draw(vcx);
    let list = bounds(vcx, "gpui-core-popover");
    vcx.simulate_event(ScrollEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-200.))),
        ..Default::default()
    });
    draw(vcx);
    draw(vcx);
    assert!(
        !shown(vcx, 0),
        "the wheel scrolled Zone 0 away, and it stays away"
    );
}
