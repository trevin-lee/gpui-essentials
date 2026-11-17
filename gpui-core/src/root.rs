//! The view at the top of a window: it sets the window's rem from the theme, draws the
//! window's surface in the theme, and moves the focus with Tab and shift-Tab.

use gpui::{
    AnyView, App, Context, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Render, Styled as _, Window, actions, div,
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
    focus: FocusHandle,
    /// The toasts shown, each with its place in the order they came.
    pub(crate) toasts: Vec<(u64, crate::toast::Toast)>,
    pub(crate) next_toast: u64,
}

impl Root {
    /// Wraps `view`. If nothing in the window has the focus yet, the Root takes it, so the
    /// first Tab has somewhere to start (with no focus at all, GPUI sends keys to no view).
    pub fn new(view: impl Into<AnyView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::init(cx);
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        if window.focused(cx).is_none() {
            window.focus(&focus, cx);
        }
        Self {
            view: view.into(),
            focus,
            toasts: Vec::new(),
            next_toast: 0,
        }
    }

    /// The content.
    pub fn view(&self) -> &AnyView {
        &self.view
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        window.set_rem_size(theme.font_size);
        let toasts = crate::toast::stack(&self.toasts, cx.entity().downgrade(), &theme);
        div()
            .id("gpui-core-root")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
            .on_action(|_: &FocusPrev, window, cx| window.focus_prev(cx))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .text_size(theme.text.base)
            .relative()
            .child(self.view.clone())
            .children(toasts)
    }
}
