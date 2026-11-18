//! A menu's destructive entry (Delete) and its emphasized one (Open): highlighted, the
//! destructive one fills with the destructive colour's fill, not the accent, and assistive tech
//! hears both labels as they are.

use gpui::{
    AppContext as _, Bounds, Context, Hsla, IntoElement, ParentElement as _, Pixels, Render, Role,
    SharedString, Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{IconName, PopupMenu, PopupMenuItem, Root, Theme};

struct Panel(SharedString);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(260.)).child(
            PopupMenu::new("menu")
                .take_focus(false)
                .highlight(self.0.clone())
                .item(PopupMenuItem::new("open", "Open").emphasized())
                .item(PopupMenuItem::new("rename", "Rename…"))
                .item(
                    PopupMenuItem::new("delete", "Delete chat…")
                        .icon(IconName::Trash)
                        .destructive(),
                ),
        )
    }
}

fn open<'a>(cx: &'a mut TestAppContext, highlighted: &str) -> &'a mut VisualTestContext {
    cx.update(gpui_core::init);
    let highlighted = SharedString::from(highlighted.to_string());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let v = cx.new(|_| Panel(highlighted));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    vcx
}

/// The solid fills drawn inside `row`.
fn fills(vcx: &mut VisualTestContext, row: Bounds<Pixels>) -> Vec<Hsla> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                let b = q.bounds;
                let (x, y) = (b.origin.x.0 / k, b.origin.y.0 / k);
                x >= f32::from(row.left()) - 1.
                    && y >= f32::from(row.top()) - 1.
                    && x + b.size.width.0 / k <= f32::from(row.right()) + 1.
                    && y + b.size.height.0 / k <= f32::from(row.bottom()) + 1.
            })
            .filter_map(|q| q.background.as_solid())
            .collect()
    })
}

fn near(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| (x - y).abs() < 0.01)
}

#[gpui::test]
fn a_highlighted_destructive_entry_fills_with_the_destructive_fill(cx: &mut TestAppContext) {
    let vcx = open(cx, "delete");
    let theme = vcx.update(|_, cx| Theme::global(cx).clone());
    let row = vcx.debug_bounds("menu-item-delete").expect("the entry");
    let f = fills(vcx, row);
    assert!(
        f.iter().any(|c| near(*c, theme.destructive_fill)),
        "its highlight: {f:?}"
    );
    assert!(
        !f.iter().any(|c| near(*c, theme.accent)),
        "not the accent: {f:?}"
    );
}

#[gpui::test]
fn another_highlighted_entry_keeps_the_accent(cx: &mut TestAppContext) {
    let vcx = open(cx, "rename");
    let theme = vcx.update(|_, cx| Theme::global(cx).clone());
    let row = vcx.debug_bounds("menu-item-rename").expect("the entry");
    let f = fills(vcx, row);
    assert!(f.iter().any(|c| near(*c, theme.accent)), "{f:?}");
    assert!(!f.iter().any(|c| near(*c, theme.destructive_fill)), "{f:?}");
}

#[gpui::test]
fn assistive_tech_hears_their_labels_as_they_are(cx: &mut TestAppContext) {
    let vcx = open(cx, "rename");
    gpui_core::audit_start();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let said = gpui_core::audit_take();
    for label in ["Open", "Delete chat…"] {
        assert!(
            said.iter()
                .any(|a| a.role == Role::MenuItem && a.label.as_deref() == Some(label)),
            "{label}: {:?}",
            said.iter().map(|a| (&a.label, a.role)).collect::<Vec<_>>()
        );
    }
}
