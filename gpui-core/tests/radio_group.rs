//! RadioGroup: a press on a circle or its label turns an option on (not a disabled one), Tab
//! stops once (on the option that's on), the arrows move and turn on what they reach, going
//! round at the ends and passing over a disabled option, a disabled group takes nothing, it's a
//! radio group to assistive tech, touch options are 44 points tall, a long label wraps inside
//! a narrow box, and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, RenderOnce,
    SharedString, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Disableable as _, Press, RadioGroup, Root, Sizable as _};

#[derive(Clone, Copy)]
struct Setup {
    touch: bool,
    disabled: bool,
    horizontal: bool,
    width: f32,
    long: bool,
}

const PLAIN: Setup = Setup {
    touch: false,
    disabled: false,
    horizontal: false,
    width: 300.,
    long: false,
};

struct Host {
    plan: SharedString,
    s: Setup,
    changes: Rc<RefCell<Vec<SharedString>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let changes = self.changes.clone();
        let mut group = RadioGroup::new("plan")
            .accessibility_label("Plan")
            .option("free", "Free")
            .option(
                "pro",
                if self.s.long {
                    "Pro, with everything in Free and unlimited agents, history and storage"
                } else {
                    "Pro"
                },
            )
            .option("custom", "Custom")
            .option("team", "Team")
            .disable("custom")
            .selected(self.plan.clone())
            .horizontal(self.s.horizontal)
            .disabled(self.s.disabled)
            .on_change(cx.listener(move |this, id: &SharedString, _, cx| {
                changes.borrow_mut().push(id.clone());
                this.plan = id.clone();
                cx.notify();
            }));
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
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            plan: "pro".into(),
            s,
            changes: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
            after: cx.focus_handle(),
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

fn plan(h: &Entity<Host>, vcx: &mut VisualTestContext) -> String {
    h.read_with(vcx, |h, _| h.plan.to_string())
}

fn changes(h: &Entity<Host>, vcx: &mut VisualTestContext) -> usize {
    h.read_with(vcx, |h, _| h.changes.borrow().len())
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = vcx.debug_bounds(selector).unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn from_before(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
}

#[gpui::test]
fn a_press_on_a_circle_or_a_label_turns_it_on_but_not_a_disabled_one(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    // The circle: the row's left edge.
    let team = vcx.debug_bounds("radio-team").unwrap();
    vcx.simulate_click(
        point(team.left() + px(8.), team.top() + px(8.)),
        Modifiers::none(),
    );
    vcx.run_until_parked();
    assert_eq!(plan(&h, vcx), "team");
    click(vcx, "radio-label-free");
    assert_eq!(plan(&h, vcx), "free");
    click(vcx, "radio-label-custom");
    assert_eq!(plan(&h, vcx), "free", "a disabled option");
    // A press on the one that's on tells the host nothing.
    click(vcx, "radio-label-free");
    assert_eq!(changes(&h, vcx), 2);
}

#[gpui::test]
fn tab_stops_once_on_the_option_thats_on(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    from_before(&h, vcx);
    keys(vcx, "tab");
    // On Pro, the one that's on: the arrows work from it.
    keys(vcx, "down");
    assert_eq!(plan(&h, vcx), "team", "Custom, disabled, passed over");
    keys(vcx, "tab");
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn the_arrows_go_round_at_the_ends(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    from_before(&h, vcx);
    keys(vcx, "tab up");
    assert_eq!(plan(&h, vcx), "free");
    keys(vcx, "up");
    assert_eq!(plan(&h, vcx), "team", "from the first, round to the last");
    keys(vcx, "right");
    assert_eq!(plan(&h, vcx), "free", "from the last, round to the first");
    keys(vcx, "left");
    assert_eq!(plan(&h, vcx), "team");
}

#[gpui::test]
fn disabled_the_group_takes_nothing(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            disabled: true,
            ..PLAIN
        },
    );
    click(vcx, "radio-label-free");
    from_before(&h, vcx);
    keys(vcx, "tab down");
    assert_eq!(plan(&h, vcx), "pro");
    assert_eq!(changes(&h, vcx), 0);
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(
        vcx.update(|window, _| after.is_focused(window)),
        "Tab passed over it"
    );
}

#[gpui::test]
fn its_a_radio_group_to_assistive_tech(cx: &mut TestAppContext) {
    struct Probe(Rc<RefCell<Option<gpui::accesskit::Node>>>);
    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let group = RadioGroup::new("g")
                .accessibility_label("Plan")
                .option("a", "A")
                .horizontal(true);
            let el = RenderOnce::render(group, window, cx).into_element();
            let role = gpui::Element::a11y_role(&el).unwrap();
            let mut node = gpui::accesskit::Node::new(role);
            gpui::Element::write_a11y_info(&el, &mut node);
            *self.0.borrow_mut() = Some(node);
            div()
        }
    }
    cx.update(gpui_core::init);
    let out = Rc::new(RefCell::new(None));
    let o = out.clone();
    let (_, vcx) = cx.add_window_view(move |_, _| Probe(o));
    vcx.run_until_parked();
    let node = out.borrow_mut().take().unwrap();
    assert_eq!(node.role(), gpui::Role::RadioGroup);
    assert_eq!(node.label(), Some("Plan"));
    assert_eq!(node.orientation(), Some(gpui::Orientation::Horizontal));
}

#[gpui::test]
fn touch_options_are_44_points_tall(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let b = vcx.debug_bounds("radio-free").unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
}

#[gpui::test]
fn side_by_side_they_share_a_row(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            horizontal: true,
            ..PLAIN
        },
    );
    let free = vcx.debug_bounds("radio-free").unwrap();
    let pro = vcx.debug_bounds("radio-pro").unwrap();
    assert_eq!(free.top(), pro.top());
    assert!(pro.left() > free.right());
}

#[cfg(target_os = "macos")]
#[test]
fn a_long_label_wraps_inside_a_narrow_box() {
    let mut cx = TestAppContext::build_with_text_system(
        gpui::TestDispatcher::new(0),
        None,
        std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    let (_, vcx) = open(
        &mut cx,
        Setup {
            width: 120.,
            long: true,
            ..PLAIN
        },
    );
    let outer = vcx.debug_bounds("box").unwrap();
    let label = vcx.debug_bounds("radio-label-pro").unwrap();
    assert!(
        label.right() <= outer.right() + px(1.),
        "{label:?} in {outer:?}"
    );
    assert!(label.size.height > px(30.), "it didn't wrap: {label:?}");
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let right = outer.right().as_f32() * scale;
        for s in &window.rendered_scene().monochrome_sprites {
            let r = s.bounds.origin.x.as_f32() + s.bounds.size.width.as_f32();
            assert!(r <= right + 2. * scale, "a glyph out at {r} past {right}");
        }
    });
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
    click(vcx, "radio-label-team");
    let settled = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
}
