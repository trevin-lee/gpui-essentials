//! A copied component next to gpui-core's original, in one app: gpui add renamed the
//! copy's actions (GPUI panics at startup on two actions with one name) and key contexts, and
//! each one's keys work. ci/check-copies.sh puts this in the crate it copies every component
//! into.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _, Render, SharedString, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Press, Root};

struct Host {
    original: Option<SharedString>,
    copy: Option<SharedString>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(Press::new("before").track_focus(&self.before).child("Before"))
            .child(
                gpui_core::Select::new("original")
                    .option("apple", "Apple")
                    .option("banana", "Banana")
                    .selected(self.original.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.original = Some(id.clone());
                        cx.notify();
                    })),
            )
            .child(
                copies::ui::Select::new("copy")
                    .option("apple", "Apple")
                    .option("banana", "Banana")
                    .selected(self.copy.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.copy = Some(id.clone());
                        cx.notify();
                    })),
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
fn a_copy_and_the_original_work_side_by_side(cx: &mut TestAppContext) {
    cx.update(|cx| {
        copies::ui::init(cx);
        let names = cx.all_action_names();
        assert!(names.iter().any(|n| n.starts_with("gpui_core_select::")));
        assert!(names.iter().any(|n| n.starts_with("ui_select::")));
    });
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            original: None,
            copy: None,
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h: Entity<Host> = made.borrow().clone().unwrap();
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));

    // Down opens each from the keyboard (its own key context's binding), and Down and Enter
    // choose Banana in it alone.
    keys(vcx, "tab down down enter");
    let chosen = |vcx: &mut VisualTestContext| {
        h.read_with(vcx, |h, _| {
            (
                h.original.as_ref().map(|s| s.to_string()),
                h.copy.as_ref().map(|s| s.to_string()),
            )
        })
    };
    assert_eq!(chosen(vcx), (Some("banana".into()), None));
    keys(vcx, "tab down down enter");
    assert_eq!(chosen(vcx), (Some("banana".into()), Some("banana".into())));
}
