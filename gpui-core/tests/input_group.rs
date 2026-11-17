//! InputGroup: a press on an addon's text or icon puts the keyboard in the text, a button
//! addon keeps its own press, typing goes into the field, the focus ring is drawn around the
//! whole group (only while its text has the keyboard), a touch one is 44 points tall, a narrow
//! space keeps everything inside the group, and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Button, IconName, InputGroup, InputState, Press, Root, Sizable as _};

#[derive(Clone, Copy)]
struct Setup {
    touch: bool,
    width: f32,
}

const PLAIN: Setup = Setup {
    touch: false,
    width: 320.,
};

struct Host {
    s: Setup,
    site: Entity<InputState>,
    copies: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let copies = self.copies.clone();
        let mut group = InputGroup::new(&self.site)
            .accessibility_label("Website")
            .leading_icon(IconName::Globe)
            .leading_text("https://")
            .trailing_text(".com")
            .trailing(
                Button::new("copy")
                    .icon(IconName::Copy)
                    .accessibility_label("Copy")
                    .xsmall()
                    .on_click(move |_, _, _| copies.set(copies.get() + 1)),
            );
        if self.s.touch {
            group = group.touch();
        }
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.s.width))
                    .debug_selector(|| "box".into())
                    .child(group),
            )
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let site = cx.new(|cx| InputState::new(window, cx));
        let h = cx.new(|cx| Host {
            s,
            site,
            copies: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
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

fn site_id(h: &Entity<Host>, vcx: &mut VisualTestContext) -> u64 {
    h.read_with(vcx, |h, _| h.site.entity_id().as_u64())
}

fn bounds(vcx: &mut VisualTestContext, name: String) -> gpui::Bounds<gpui::Pixels> {
    let name: &'static str = Box::leak(name.into_boxed_str());
    vcx.debug_bounds(name)
        .unwrap_or_else(|| panic!("{name} drawn"))
}

fn focused(h: &Entity<Host>, vcx: &mut VisualTestContext) -> bool {
    let site = h.read_with(vcx, |h, _| h.site.clone());
    vcx.update(|window, cx| site.read(cx).is_focused(window))
}

#[gpui::test]
fn a_press_on_an_addon_puts_the_keyboard_in_the_text(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    let id = site_id(&h, vcx);
    let leading = bounds(vcx, format!("input-group-leading-{id}"));
    vcx.simulate_click(leading.center(), Modifiers::none());
    vcx.run_until_parked();
    assert!(focused(&h, vcx), "a press on https:// focused the text");
    vcx.simulate_input("example");
    assert_eq!(
        h.read_with(vcx, |h, cx| h.site.read(cx).value().to_string()),
        "example"
    );
}

#[gpui::test]
fn a_button_addon_keeps_its_own_press(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    let id = site_id(&h, vcx);
    let trailing = bounds(vcx, format!("input-group-trailing-{id}"));
    // The button is the trailing side's last 24 points.
    let at = gpui::point(trailing.right() - px(10.), trailing.center().y);
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(h.read_with(vcx, |h, _| h.copies.get()), 1);
    assert!(!focused(&h, vcx), "the button's press went to the text too");
}

/// The shadows drawn whose element is the group (its ring), in points.
fn group_shadows(h: &Entity<Host>, vcx: &mut VisualTestContext) -> usize {
    let id = site_id(h, vcx);
    let group = bounds(vcx, format!("input-group-{id}"));
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        window
            .rendered_scene()
            .shadows
            .iter()
            .filter(|s| {
                let b = s.element_bounds;
                (b.origin.x.as_f32() / scale - group.left().as_f32()).abs() < 1.
                    && (b.size.width.as_f32() / scale - group.size.width.as_f32()).abs() < 1.
            })
            .count()
    })
}

#[gpui::test]
fn the_focus_ring_goes_round_the_whole_group(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    assert_eq!(
        group_shadows(&h, vcx),
        0,
        "a ring before it had the keyboard"
    );
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
    assert!(focused(&h, vcx));
    assert_eq!(group_shadows(&h, vcx), 1, "no ring round the group");
    keys(vcx, "shift-tab");
    assert_eq!(group_shadows(&h, vcx), 0, "the ring stayed");
}

#[gpui::test]
fn a_touch_one_is_44_points_tall(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let id = site_id(&h, vcx);
    let b = bounds(vcx, format!("input-group-{id}"));
    assert!(b.size.height >= px(44.), "{b:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn a_narrow_space_keeps_everything_inside() {
    for width in [60., 120., 200.] {
        let mut cx = TestAppContext::build_with_text_system(
            gpui::TestDispatcher::new(0),
            None,
            std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        let (h, vcx) = open(&mut cx, Setup { width, ..PLAIN });
        let id = site_id(&h, vcx);
        let outer = vcx.debug_bounds("box").unwrap();
        let group = bounds(vcx, format!("input-group-{id}"));
        assert!(
            group.right() <= outer.right() + px(1.),
            "{group:?} in {outer:?}"
        );
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            let right = outer.right().as_f32() * scale;
            let scene = window.rendered_scene();
            for s in &scene.monochrome_sprites {
                let clip = s.content_mask.bounds;
                let shown = (s.bounds.origin.x.as_f32() + s.bounds.size.width.as_f32())
                    .min(clip.origin.x.as_f32() + clip.size.width.as_f32());
                assert!(
                    shown <= right + 2. * scale,
                    "at {width}: a glyph out at {shown} past {right}"
                );
            }
        });
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_nothing_changes(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
    vcx.simulate_input("abc");
    vcx.run_until_parked();
    let settled = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
}
