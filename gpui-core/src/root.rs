//! The view at the top of a window: it sets the window's rem from the theme, draws the
//! window's surface in the theme, and moves the focus with Tab and shift-Tab.

use gpui::{
    AnyView, App, Context, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _,
    Render, Styled as _, Window, actions, div,
};

use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core,
    [
        /// Moves the focus to the next thing that takes it.
        FocusNext,
        /// Moves the focus to the previous thing that takes it.
        FocusPrev
    ]
);

const CONTEXT: &str = "GpuiCoreRoot";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, Some(CONTEXT)),
        KeyBinding::new("shift-tab", FocusPrev, Some(CONTEXT)),
    ]);
}

/// Wraps a window's content. On every draw it makes the theme's
/// [`font_size`](Theme::font_size) the window's rem, so every size in rems follows the theme
/// (the host's zoom); it draws the theme's background, text colour and font under the content;
/// and Tab and shift-Tab move the focus. It redraws when the theme changes, and only then.
pub struct Root {
    view: AnyView,
}

impl Root {
    pub fn new(view: impl Into<AnyView>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self { view: view.into() }
    }

    /// The content.
    pub fn view(&self) -> &AnyView {
        &self.view
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        window.set_rem_size(theme.font_size);
        div()
            .id("gpui-core-root")
            .key_context(CONTEXT)
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrev, window, cx| window.focus_prev(cx))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .text_size(theme.text.base)
            .child(self.view.clone())
    }
}
