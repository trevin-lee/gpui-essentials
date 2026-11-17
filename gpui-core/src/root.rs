//! The view at the top of a window: it sets the window's rem from the theme, draws the
//! window's surface in the theme, and moves the focus with Tab and shift-Tab.

use gpui::{
    AnyView, App, Context, Entity, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, Styled as _, Window, actions, div,
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
    ///
    /// Keys then go to the Root, which is above `view`: a key binding in `view`'s own key
    /// context (a ⌘K palette, say) does nothing until something inside `view` has the focus.
    /// A view with key bindings of its own wants [`Root::focusing`] (or
    /// [`open_window_focusing`](crate::open_window_focusing)) instead.
    pub fn new(view: impl Into<AnyView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_first_focus(view.into(), None, window, cx)
    }

    /// Wraps `view`, as [`Root::new`] does, but if nothing in the window has the focus yet,
    /// `view` takes it rather than the Root: its key bindings work from the first key, and Tab
    /// and shift-Tab still move the focus. `view`'s [`focus_handle`](Focusable::focus_handle)
    /// must be tracked by an element it draws (`track_focus`), with its key context on that
    /// element or above it.
    pub fn focusing<V: Render + Focusable>(
        view: Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let first = view.read(cx).focus_handle(cx);
        Self::with_first_focus(view.into(), Some(first), window, cx)
    }

    fn with_first_focus(
        view: AnyView,
        first: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::init(cx);
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        if window.focused(cx).is_none() {
            window.focus(first.as_ref().unwrap_or(&focus), cx);
        }
        Self {
            view,
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
