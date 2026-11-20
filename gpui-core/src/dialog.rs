//! A dialog: a modal panel in the middle of the window over a scrim, with a title, a
//! description, content, a footer of buttons and a close button; and an alert dialog, the same
//! asking for a decision, which a press on the scrim doesn't close.
//!
//! The host owns whether it's open, as with a [`Popover`](crate::base::Popover): it draws
//! `Dialog::new(id).open(open)` every frame and closes it from [`Dialog::on_dismiss`], which runs
//! on Escape, a press on the scrim and the close button.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, ButtonVariants as _, Dialog};
//! # struct Panel { open: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div().size_full().child(
//!         Dialog::new("rename")
//!             .open(self.open)
//!             .title("Rename the project")
//!             .description("Its new name shows everywhere it does.")
//!             .on_dismiss(cx.listener(|this, _, _, cx| {
//!                 this.open = false;
//!                 cx.notify();
//!             }))
//!             .child("The form")
//!             .footer(Button::new("save").label("Save").on_click(cx.listener(
//!                 |this, _, _, cx| {
//!                     this.open = false;
//!                     cx.notify();
//!                 },
//!             ))),
//!     )
//! }
//! # }
//! ```

use gpui::{
    AnyElement, App, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement, Rems, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, relative, rems,
};

use crate::base::{
    ActiveTheme as _, Dismiss, DismissReason, Icon, IconName, MODAL_CONTEXT, ModalElement,
    POPOVER_CONTEXT, PopoverColors, Sizable as _, TrapNext, TrapPrev, cycle_focus, focus_first_in,
    just_opened, overlay_focus,
};
use crate::button::{Button, ButtonVariants as _};

/// Runs on a dismiss, with why.
type DismissFn = std::rc::Rc<dyn Fn(&DismissReason, &mut Window, &mut App)>;

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// The web's `sm` breakpoint: narrower than this, a footer's buttons stack.
const NARROW: f32 = 640.;

/// A dialog: a panel in the middle of the window, over a scrim that dims and blocks the rest.
/// It's at most [`max_width`](Dialog::max_width) wide (and 16 pixels in from each side of a
/// narrower window), as tall as its content, and scrolls when that's taller than the window.
/// It's drawn only while open, over popovers; toasts are drawn over it.
///
/// While open it has the keyboard: it goes to [`initial_focus`](Dialog::initial_focus), or to
/// the first thing inside that takes the focus, or to the dialog itself; Tab and shift-Tab go
/// round what's inside it and never leave it. When it closes, the focus goes back to what had
/// it when it opened, or to [`return_focus`](Dialog::return_focus). For that, keep drawing the
/// dialog while it's closed: closed, it draws nothing.
///
/// To assistive tech it's a dialog named by its title and described by its description.
#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    open: bool,
    title: Option<SharedString>,
    description: Option<SharedString>,
    footer: Vec<AnyElement>,
    on_dismiss: Option<DismissFn>,
    close_button: bool,
    dismiss_on_scrim: bool,
    role: Role,
    initial_focus: Option<FocusHandle>,
    /// An alert dialog's: the first thing in the footer (its Cancel) takes the keyboard first.
    footer_first: bool,
    return_focus: Option<FocusHandle>,
    max_width: Rems,
    colors: Option<PopoverColors>,
    scrim: Option<Hsla>,
    touch: bool,
    label: Option<SharedString>,
    /// No padding round the content (a command palette's field and list reach the edges).
    bare: bool,
    /// Its top at a fixed place rather than its middle in the window's.
    anchor_top: bool,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Dialog {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            title: None,
            description: None,
            footer: Vec::new(),
            on_dismiss: None,
            close_button: true,
            dismiss_on_scrim: true,
            role: Role::Dialog,
            initial_focus: None,
            footer_first: false,
            return_focus: None,
            // The web's `sm:max-w-sm`.
            max_width: css(384.),
            colors: None,
            scrim: None,
            touch: false,
            label: None,
            bare: false,
            anchor_top: false,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Its top at a fixed place, about a fifth of the way down the window (18%, at most 160
    /// points), where it's otherwise in the middle: a dialog whose height changes as it's used
    /// (a command palette's list as you type) stays put and grows downward, as Spotlight's does.
    pub fn anchor_top(mut self, anchor: bool) -> Self {
        self.anchor_top = anchor;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Its title, at the top: what it's called to assistive tech too. It wraps.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A line or two under the title, in the muted colour. It wraps.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds an element (a button) to the footer, a row along the bottom, on the right, in the
    /// order given; on a narrow window they stack, the last on top.
    pub fn footer(mut self, element: impl IntoElement) -> Self {
        self.footer.push(element.into_any_element());
        self
    }

    /// Runs on Escape, on a press on the scrim and on the close button, with which it was. The
    /// host closes it here.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(std::rc::Rc::new(f));
        self
    }

    /// Whether it has a close button (an ×) at its top right (it has by default).
    pub fn close_button(mut self, shown: bool) -> Self {
        self.close_button = shown;
        self
    }

    /// What has the keyboard when it opens, instead of the first thing inside it that takes it.
    pub fn initial_focus(mut self, handle: &FocusHandle) -> Self {
        self.initial_focus = Some(handle.clone());
        self
    }

    /// Where the focus goes when it closes, instead of what had it when it opened.
    pub fn return_focus(mut self, handle: &FocusHandle) -> Self {
        self.return_focus = Some(handle.clone());
        self
    }

    /// The widest it draws (24 rem by default, the web's).
    pub fn max_width(mut self, width: Rems) -> Self {
        self.max_width = width;
        self
    }

    /// The panel's colours: by default the theme's `popover`, `popover-foreground` and
    /// `border`.
    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// The scrim's colour: by default the theme's `background` at 70%.
    pub fn scrim(mut self, color: impl Into<Hsla>) -> Self {
        self.scrim = Some(color.into());
        self
    }

    /// What it's called to assistive tech, when it has no [`title`](Self::title) (or for a
    /// fuller name than that).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Its content reaches its edges: no padding round it, and no gap between the title, the
    /// description, the content and the footer (a command palette's field and list).
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// Makes the close button 44 points each way, for a finger.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The content's own style (between the description and the footer): gap, and so on.
impl Styled for Dialog {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Dialog {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let opened = just_opened(&self.id, self.open, window, cx);
        let focus = overlay_focus(
            &self.id,
            self.open,
            true,
            self.return_focus.clone(),
            window,
            cx,
        );
        let footer_focus = window
            .use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "footer".into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        if opened {
            // After the overlay's own focusing, and with this frame's tab stops drawn.
            let initial = self.initial_focus.clone();
            let footer_first = self.footer_first;
            let (panel, footer) = (focus.clone(), footer_focus.clone());
            window.defer(cx, move |window, cx| match initial {
                Some(to) => window.focus(&to, cx),
                None => {
                    if !(footer_first && focus_first_in(&footer, window, cx)) {
                        focus_first_in(&panel, window, cx);
                    }
                }
            });
        }
        let scrim = self
            .open
            .then(|| self.build(&focus, &footer_focus, window, cx));
        ModalElement::new(scrim)
    }
}

impl Dialog {
    fn build(
        &mut self,
        focus: &FocusHandle,
        footer_focus: &FocusHandle,
        window: &Window,
        cx: &App,
    ) -> Stateful<gpui::Div> {
        let theme = cx.theme();
        let colors = self
            .colors
            .unwrap_or_else(|| PopoverColors::from_theme(theme));
        let narrow = window.viewport_size().width < window.rem_size() * (NARROW / 15.);
        let alert = self.role == Role::AlertDialog;

        let header = (self.title.is_some() || self.description.is_some()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(css(if alert { 6. } else { 8. }))
                .min_w_0()
                // Clear of the close button.
                .when(self.close_button, |h| h.pr(css(28.)))
                .when(alert && narrow, |h| h.items_center().text_center())
                .when_some(self.title.clone(), |h, title| {
                    h.child(
                        div()
                            .debug_selector(|| "gpui-core-dialog-title".into())
                            .text_size(theme.text.base)
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(relative(1.25))
                            .child(title),
                    )
                })
                .when_some(self.description.clone(), |h, description| {
                    h.child(
                        div()
                            .text_size(theme.text.sm)
                            .text_color(theme.muted_foreground)
                            .child(description),
                    )
                })
        });
        let mut body = div().flex().flex_col().min_w_0();
        gpui::Refineable::refine(body.style(), &self.style);
        let children = std::mem::take(&mut self.children);
        let body = (!children.is_empty()).then(|| body.children(children));
        let footer = std::mem::take(&mut self.footer);
        let footer = (!footer.is_empty()).then(|| {
            div()
                .id("dialog-footer")
                .track_focus(footer_focus)
                .flex()
                .gap(css(8.))
                .map(|f| {
                    if narrow {
                        f.flex_col_reverse()
                    } else {
                        f.flex_row().justify_end()
                    }
                })
                .mx(css(-16.))
                .mb(css(-16.))
                .p(css(16.))
                .border_t_1()
                .border_color(colors.border)
                .bg(theme.muted.opacity(0.5))
                .children(footer)
        });
        let scroll = div()
            .id("dialog-scroll")
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .when(!self.bare, |s| s.p(css(16.)).gap(css(16.)))
            .children(header)
            .children(body)
            .children(footer);

        let on_close = self.on_dismiss.clone();
        let close = self.close_button.then(|| {
            Button::new("gpui-core-dialog-close")
                .ghost()
                .icon(Icon::new(IconName::X))
                .accessibility_label("Close")
                .map(|b| if self.touch { b.touch() } else { b.small() })
                .on_click(move |_, window, cx| {
                    if let Some(f) = &on_close {
                        f(&DismissReason::CloseButton, window, cx);
                    }
                })
        });
        let close = close.map(|b| {
            div()
                .absolute()
                .top(css(8.))
                .right(css(8.))
                .debug_selector(|| "gpui-core-dialog-close".into())
                .child(b)
        });

        let on_escape = self.on_dismiss.clone();
        let mut panel = div()
            .id("dialog")
            .role(self.role)
            .key_context(POPOVER_CONTEXT)
            .track_focus(focus)
            // Over the scrim, so a press inside isn't one on it.
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .max_w(self.max_width)
            .max_h_full()
            .overflow_hidden()
            .rounded(theme.radius_xl())
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_color(colors.foreground)
            .text_size(theme.text.sm)
            .shadow_lg()
            .debug_selector(|| "gpui-core-dialog".into())
            .on_action(move |_: &Dismiss, window, cx| {
                if let Some(f) = &on_escape {
                    f(&DismissReason::Escape, window, cx);
                }
            })
            .child(scroll)
            .children(close);
        if let Some(label) = self.label.clone().or_else(|| self.title.clone()) {
            panel = panel.aria_label(label);
        }
        if let Some(description) = self.description.clone() {
            panel = panel.aria_description(description);
        }

        // Anchored at its top: a fifth of the way down, at most 160 points, at least the 16 the
        // other sides keep.
        let anchor_top = self.anchor_top;
        let top = (window.viewport_size().height * 0.18)
            .min(gpui::px(160.))
            .max(css(16.).to_pixels(window.rem_size()));
        let on_scrim = self
            .dismiss_on_scrim
            .then(|| self.on_dismiss.clone())
            .flatten();
        let (next, prev) = (focus.clone(), focus.clone());
        div()
            .id(self.id.clone())
            .key_context(MODAL_CONTEXT)
            .occlude()
            .flex()
            .justify_center()
            .p(css(16.))
            .map(|d| {
                if anchor_top {
                    d.items_start().pt(top)
                } else {
                    d.items_center()
                }
            })
            .bg(self.scrim.unwrap_or_else(|| theme.background.opacity(0.7)))
            .debug_selector(|| "gpui-core-dialog-scrim".into())
            .on_action(move |_: &TrapNext, window, cx| cycle_focus(&next, true, window, cx))
            .on_action(move |_: &TrapPrev, window, cx| cycle_focus(&prev, false, window, cx))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if let Some(f) = &on_scrim {
                    f(&DismissReason::ClickOutside, window, cx);
                }
            })
            .child(panel)
    }
}

/// An alert dialog: a [`Dialog`] that asks for a decision, with a Cancel and an action in its
/// footer. It has no close button and a press on the scrim doesn't close it; Escape does
/// ([`on_dismiss`](AlertDialog::on_dismiss)), as Cancel should. When it opens, the keyboard goes
/// to the first thing in its footer, the least destructive choice: its
/// [`cancel`](AlertDialog::cancel). To assistive tech it's an alert dialog.
///
/// ```no_run
/// # use gpui::*;
/// # use gpui_core::{AlertDialog, Button, ButtonVariants as _};
/// # struct Panel { open: bool }
/// # impl Render for Panel {
/// fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
///     let close = cx.listener(|this: &mut Panel, _: &ClickEvent, _: &mut Window, cx| {
///         this.open = false;
///         cx.notify();
///     });
///     div().size_full().child(
///         AlertDialog::new("delete")
///             .open(self.open)
///             .title("Delete the project?")
///             .description("Its files go with it. This can't be undone.")
///             .on_dismiss(cx.listener(|this, _, _, cx| {
///                 this.open = false;
///                 cx.notify();
///             }))
///             .cancel(Button::new("cancel").outline().label("Cancel").on_click(close))
///             .action(Button::new("delete").destructive().label("Delete")),
///     )
/// }
/// # }
/// ```
#[derive(IntoElement)]
pub struct AlertDialog {
    dialog: Dialog,
    cancel: Option<AnyElement>,
    action: Vec<AnyElement>,
}

impl AlertDialog {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let mut dialog = Dialog::new(id).close_button(false);
        dialog.dismiss_on_scrim = false;
        dialog.role = Role::AlertDialog;
        dialog.footer_first = true;
        Self {
            dialog,
            cancel: None,
            action: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.dialog = self.dialog.open(open);
        self
    }

    /// The question, at the top: what it's called to assistive tech too.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.dialog = self.dialog.title(title);
        self
    }

    /// What follows from the choice, under the title.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.dialog = self.dialog.description(description);
        self
    }

    /// The choice that changes nothing (an outline "Cancel" button): first in the footer, and
    /// what has the keyboard when it opens. The host closes the dialog from its press.
    pub fn cancel(mut self, element: impl IntoElement) -> Self {
        self.cancel = Some(element.into_any_element());
        self
    }

    /// The action (a button, destructive where it deletes), after the cancel. More than one go
    /// in the order given.
    pub fn action(mut self, element: impl IntoElement) -> Self {
        self.action.push(element.into_any_element());
        self
    }

    /// Runs on Escape. The host closes it here, as from the cancel.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.dialog = self.dialog.on_dismiss(f);
        self
    }

    /// Where the focus goes when it closes, instead of what had it when it opened.
    pub fn return_focus(mut self, handle: &FocusHandle) -> Self {
        self.dialog = self.dialog.return_focus(handle);
        self
    }

    /// The widest it draws (24 rem by default).
    pub fn max_width(mut self, width: Rems) -> Self {
        self.dialog = self.dialog.max_width(width);
        self
    }

    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.dialog = self.dialog.colors(colors);
        self
    }

    pub fn scrim(mut self, color: impl Into<Hsla>) -> Self {
        self.dialog = self.dialog.scrim(color);
        self
    }
}

impl ParentElement for AlertDialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.dialog.extend(elements)
    }
}

impl RenderOnce for AlertDialog {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut dialog = self.dialog;
        dialog.footer.extend(self.cancel);
        dialog.footer.extend(self.action);
        dialog
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AppContext as _, Context, IntoElement, Render, TestAppContext, Window, canvas, div, px,
    };

    use super::*;
    use crate::base::{MODAL_PRIORITY, OVERLAY_PRIORITY, Popover, Root, overlay_layer};

    /// A dialog holding an open popover, each with a probe that records the layer it's placed in.
    struct Host {
        layers: Rc<RefCell<Vec<(&'static str, usize)>>>,
    }

    fn probe(name: &'static str, layers: &Rc<RefCell<Vec<(&'static str, usize)>>>) -> AnyElement {
        let layers = layers.clone();
        canvas(
            move |_, _, cx| layers.borrow_mut().push((name, overlay_layer(cx))),
            |_, _, _, _| {},
        )
        .size(px(10.))
        .into_any_element()
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(probe("page", &self.layers)).child(
                Dialog::new("dialog")
                    .open(true)
                    .child(probe("dialog", &self.layers))
                    .child(
                        Popover::new("inner")
                            .open(true)
                            .trigger(div().size(px(20.)))
                            .child(probe("popover", &self.layers)),
                    ),
            )
        }
    }

    #[gpui::test]
    fn what_a_dialog_draws_goes_over_it(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let layers: Rc<RefCell<Vec<(&'static str, usize)>>> = Rc::default();
        let l = layers.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let host = cx.new(|_| Host { layers: l });
            Root::new(host, window, cx)
        });
        vcx.run_until_parked();
        let seen = layers.borrow().clone();
        let last = |name| seen.iter().rev().find(|(n, _)| *n == name).map(|(_, l)| *l);
        assert_eq!(last("page"), Some(OVERLAY_PRIORITY));
        assert_eq!(last("dialog"), Some(MODAL_PRIORITY));
        // Drawn from inside the dialog, so in its layer and over it.
        assert_eq!(last("popover"), Some(MODAL_PRIORITY));
    }
}
