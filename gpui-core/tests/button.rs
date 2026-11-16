//! Button and ButtonGroup: presses reach the handler, a group says which button was pressed,
//! disabled ones ignore presses, and the touch size is at least 44 points each way.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _,
    Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{
    Button, ButtonGroup, ButtonVariants as _, Disableable as _, IconName, Root, Selectable as _,
    Sizable as _,
};

type Log = Rc<RefCell<Vec<String>>>;

struct Panel {
    log: Log,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = |what: &'static str, l: &Log| {
            let l = l.clone();
            move |_: &_, _: &mut Window, _: &mut gpui::App| l.borrow_mut().push(what.into())
        };
        let group_log = self.log.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .child(
                div().debug_selector(|| "save".into()).child(
                    Button::new("save")
                        .label("Save")
                        .on_click(log("save", &self.log)),
                ),
            )
            .child(
                div().debug_selector(|| "off".into()).child(
                    Button::new("off")
                        .label("Off")
                        .disabled(true)
                        .on_click(log("off", &self.log)),
                ),
            )
            .child(
                div().debug_selector(|| "touch".into()).child(
                    Button::new("touch")
                        .icon(IconName::Plus)
                        .accessibility_label("Add")
                        .ghost()
                        .touch(),
                ),
            )
            .child(
                ButtonGroup::new("views")
                    .outline()
                    .small()
                    .children(
                        ["Day", "Week", "Month"]
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| Button::new(("view", i)).label(v).selected(i == 1)),
                    )
                    .on_click(move |picked: &Vec<usize>, _, _| {
                        group_log.borrow_mut().push(format!("view {:?}", picked))
                    }),
            )
            .child(div().debug_selector(|| "end".into()).h(px(1.)))
    }
}

fn open(cx: &mut TestAppContext) -> (Log, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let log: Log = Rc::default();
    let l = log.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel { log: l });
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    (log, vcx)
}

fn click(vcx: &mut VisualTestContext, which: &'static str) {
    let at = vcx.debug_bounds(which).expect("drawn").center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn a_press_reaches_the_handler_unless_disabled(cx: &mut TestAppContext) {
    let (log, vcx) = open(cx);
    click(vcx, "save");
    click(vcx, "off");
    assert_eq!(*log.borrow(), ["save"]);
}

#[gpui::test]
fn a_group_says_which_button_was_pressed(cx: &mut TestAppContext) {
    let (log, vcx) = open(cx);
    // The group sits above the end marker; its buttons share its row, Day first.
    let end = vcx.debug_bounds("end").unwrap();
    let touch = vcx.debug_bounds("touch").unwrap();
    let row_y = (touch.bottom() + end.top()) / 2.;
    let mut x = px(2.);
    let mut seen = Vec::new();
    while x < px(400.) {
        vcx.simulate_click(gpui::point(x, row_y), Modifiers::none());
        vcx.run_until_parked();
        if let Some(last) = log.borrow().last()
            && seen.last() != Some(last)
        {
            seen.push(last.clone());
        }
        x += px(6.);
    }
    assert_eq!(seen, ["view [0]", "view [1]", "view [2]"]);
}

#[gpui::test]
fn the_touch_size_is_at_least_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let b = vcx.debug_bounds("touch").unwrap();
    assert!(
        b.size.width >= px(44.) && b.size.height >= px(44.),
        "{:?}",
        b.size
    );
}
