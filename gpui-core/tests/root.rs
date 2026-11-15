//! The window's Root: the rem follows the theme, Tab moves the focus, a theme change redraws
//! and nothing else does.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Root, Theme};

struct Content {
    draws: Rc<Cell<usize>>,
    fields: Vec<FocusHandle>,
}

impl Render for Content {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().size_full().children(
            self.fields
                .iter()
                .enumerate()
                .map(|(i, f)| div().id(i).track_focus(f).w(px(40.)).h(px(20.))),
        )
    }
}

impl Focusable for Content {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.fields[0].clone()
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Content>, Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let made = Rc::new(Cell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let content = cx.new(|cx| Content {
            draws: d,
            fields: (0..3).map(|_| cx.focus_handle().tab_stop(true)).collect(),
        });
        m.set(Some(content.clone()));
        Root::new(content, window, cx)
    });
    vcx.run_until_parked();
    (made.take().unwrap(), draws, vcx)
}

#[gpui::test]
fn the_rem_is_the_themes_font_size(cx: &mut TestAppContext) {
    let (_, _, vcx) = open(cx);
    vcx.update(|window, _| assert_eq!(window.rem_size(), px(15.)));
    // A host zooming to 200%.
    vcx.update(|_, cx| Theme::global_mut(cx).font_size = px(30.));
    vcx.run_until_parked();
    vcx.update(|window, _| assert_eq!(window.rem_size(), px(30.)));
}

#[gpui::test]
fn a_theme_change_redraws_and_nothing_else_does(cx: &mut TestAppContext) {
    let (_, draws, vcx) = open(cx);
    let settled = draws.get();
    assert!(settled > 0);
    vcx.executor().advance_clock(Duration::from_secs(10));
    vcx.run_until_parked();
    assert_eq!(draws.get(), settled, "drew again with nothing changed");
    vcx.update(|_, cx| Theme::set(Theme::light(), cx));
    vcx.run_until_parked();
    assert!(draws.get() > settled, "didn't draw the new theme");
}

#[gpui::test]
fn tab_and_shift_tab_move_the_focus(cx: &mut TestAppContext) {
    let (content, _, vcx) = open(cx);
    let fields = content.read_with(vcx, |c, _| c.fields.clone());
    vcx.update(|window, cx| window.focus(&fields[0], cx));
    vcx.simulate_keystrokes("tab");
    vcx.update(|window, _| assert!(fields[1].is_focused(window)));
    vcx.simulate_keystrokes("tab");
    vcx.update(|window, _| assert!(fields[2].is_focused(window)));
    vcx.simulate_keystrokes("shift-tab shift-tab");
    vcx.update(|window, _| assert!(fields[0].is_focused(window)));
}
