//! Sheet: across the bottom of the window, centred and capped on a wide one, closed by a press
//! on the scrim or Escape, giving the focus back, drawing nothing while closed and nothing
//! again while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{DismissReason, Press, Root, Sheet};

struct Panel {
    open: bool,
    rows: usize,
    opener: FocusHandle,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let reasons = self.reasons.clone();
        div()
            .size_full()
            .child(
                Press::new("opener")
                    .track_focus(&self.opener)
                    .debug_selector(|| "opener".into())
                    .size(px(40.))
                    .child("Open")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Sheet::new("sheet")
                    .open(self.open)
                    .on_dismiss(cx.listener(move |this, reason: &DismissReason, _, cx| {
                        reasons.borrow_mut().push(*reason);
                        this.open = false;
                        cx.notify();
                    }))
                    .children((0..self.rows).map(|i| div().h(px(44.)).child(format!("Row {i}")))),
            )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, rows: usize, width: f32) -> Opened<'_> {
    cx.update(gpui_core::init);
    let reasons: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (r, d) = (reasons.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            open: false,
            rows,
            opener: cx.focus_handle(),
            reasons: r,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(width), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        reasons,
        draws,
        vcx,
    }
}

fn sheet(vcx: &mut VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    vcx.debug_bounds("gpui-core-sheet")
}

fn press_opener(o: &mut Opened<'_>) {
    let at = o.vcx.debug_bounds("opener").unwrap().center();
    o.vcx.simulate_click(at, Modifiers::none());
    o.vcx.run_until_parked();
}

fn key(vcx: &mut VisualTestContext, k: &'static str) {
    let keystroke = Keystroke::parse(k).unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

#[gpui::test]
fn it_sits_across_the_bottom(cx: &mut TestAppContext) {
    let mut o = open(cx, 3, 400.);
    assert!(sheet(o.vcx).is_none(), "closed, nothing drawn");
    press_opener(&mut o);
    let s = sheet(o.vcx).expect("open");
    assert_eq!(s.bottom(), px(600.));
    assert_eq!(s.left(), px(0.));
    assert_eq!(s.size.width, px(400.));
}

#[gpui::test]
fn on_a_wide_window_its_capped_and_centred(cx: &mut TestAppContext) {
    let mut o = open(cx, 3, 1400.);
    press_opener(&mut o);
    let s = sheet(o.vcx).expect("open");
    assert!(s.size.width < px(1400.), "{s:?}");
    assert!((s.center().x - px(700.)).abs() < px(1.), "{s:?}");
}

#[gpui::test]
fn tall_content_scrolls_below_nine_tenths(cx: &mut TestAppContext) {
    let mut o = open(cx, 60, 400.);
    press_opener(&mut o);
    let s = sheet(o.vcx).expect("open");
    assert!(s.size.height <= px(540.5), "{s:?}");
    assert_eq!(s.bottom(), px(600.));
}

#[gpui::test]
fn a_press_on_the_scrim_dismisses_it_and_inside_doesnt(cx: &mut TestAppContext) {
    let mut o = open(cx, 3, 400.);
    press_opener(&mut o);
    let s = sheet(o.vcx).unwrap();
    o.vcx.simulate_click(s.center(), Modifiers::none());
    assert!(o.reasons.borrow().is_empty());
    // The opener is under the scrim: the press is the scrim's, not the opener's.
    o.vcx
        .simulate_click(point(px(20.), px(20.)), Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(*o.reasons.borrow(), [DismissReason::ClickOutside]);
    assert!(sheet(o.vcx).is_none());
}

#[gpui::test]
fn escape_dismisses_it_and_the_focus_goes_back(cx: &mut TestAppContext) {
    let o = open(cx, 3, 400.);
    key(o.vcx, "tab");
    let opener = o.panel.read_with(o.vcx, |p, _| p.opener.clone());
    assert!(o.vcx.update(|w, _| opener.is_focused(w)));
    key(o.vcx, "space");
    assert!(sheet(o.vcx).is_some());
    assert!(
        !o.vcx.update(|w, _| opener.is_focused(w)),
        "the sheet has the keyboard"
    );
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
    assert!(o.vcx.update(|w, _| opener.is_focused(w)));
}

#[gpui::test]
fn open_and_idle_it_doesnt_redraw(cx: &mut TestAppContext) {
    let mut o = open(cx, 3, 400.);
    press_opener(&mut o);
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx
            .executor()
            .advance_clock(std::time::Duration::from_millis(16));
        o.vcx.run_until_parked();
    }
    assert_eq!(o.draws.get(), before);
}
