//! A drawer: a modal panel that comes from an edge of the window (the bottom by default; the
//! top, left or right too) over a scrim, with a title, a description, content and a footer.
//!
//! The host owns whether it's open, as with a `Dialog`: it draws
//! `Drawer::new(id).open(open)` every frame and closes it from [`Drawer::on_dismiss`], which runs
//! on Escape, a press on the scrim, and, for a bottom drawer, a drag down past a quarter of its
//! height (a finger pulling it away).
//!
//! It's `Sheet`'s fuller form: a Sheet is a plain bottom panel of touch rows
//! (an action sheet), with no title, footer or drag, which doesn't hold Tab inside it; a Drawer
//! comes from any edge, has the header and footer, keeps Tab inside it as a dialog does, and a
//! bottom one is dragged away.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, Drawer};
//! # struct Panel { open: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div().size_full().child(
//!         Drawer::new("goal")
//!             .open(self.open)
//!             .title("Move goal")
//!             .description("Set your daily activity goal.")
//!             .on_dismiss(cx.listener(|this, _, _, cx| {
//!                 this.open = false;
//!                 cx.notify();
//!             }))
//!             .child("350 calories a day")
//!             .footer(Button::new("submit").label("Submit")),
//!     )
//! }
//! # }
//! ```

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Rems,
    RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Window, canvas, div, prelude::FluentBuilder as _, px, relative, rems,
};

use crate::base::{
    ActiveTheme as _, Dismiss, DismissReason, MODAL_CONTEXT, ModalElement, POPOVER_CONTEXT,
    PopoverColors, TrapNext, TrapPrev, cycle_focus, focus_first_in, just_opened, overlay_focus,
};

/// Runs on a dismiss, with why.
type DismissFn = std::rc::Rc<dyn Fn(&DismissReason, &mut Window, &mut App)>;

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// How far down (as a share of its height) a bottom drawer is dragged before letting go
/// dismisses it; short of that, it goes back.
pub const DRAWER_DISMISS_FRACTION: f32 = 0.25;

/// How far a press moves before it's a drag, so a tap on the handle isn't one.
const SLOP: f32 = 4.;

/// The edge a [`Drawer`] comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DrawerSide {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

/// A panel against an edge of the window, over a scrim that dims and blocks the rest. A bottom
/// or top drawer is as wide as the window and as tall as its content up to four fifths of the
/// window; a left or right one is as tall as the window and three quarters of its width up to
/// [`max_width`](Drawer::max_width). Its content scrolls past that. A bottom drawer has a grab
/// handle, and its handle and header can be dragged down to dismiss it. It's drawn only while
/// open, over popovers.
///
/// While open it has the keyboard, as a `Dialog`: the first thing inside takes
/// it (or [`initial_focus`](Drawer::initial_focus)), Tab and shift-Tab go round inside, and on
/// closing the focus goes back. Keep drawing it while it's closed for that.
#[derive(IntoElement)]
pub struct Drawer {
    id: ElementId,
    open: bool,
    side: DrawerSide,
    title: Option<SharedString>,
    description: Option<SharedString>,
    footer: Vec<AnyElement>,
    on_dismiss: Option<DismissFn>,
    initial_focus: Option<FocusHandle>,
    return_focus: Option<FocusHandle>,
    max_width: Rems,
    colors: Option<PopoverColors>,
    scrim: Option<Hsla>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Drawer {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            side: DrawerSide::Bottom,
            title: None,
            description: None,
            footer: Vec::new(),
            on_dismiss: None,
            initial_focus: None,
            return_focus: None,
            // The web's `sm:max-w-sm`, for a side drawer.
            max_width: css(384.),
            colors: None,
            scrim: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The edge it comes from (the bottom by default).
    pub fn side(mut self, side: DrawerSide) -> Self {
        self.side = side;
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

    /// Adds an element (a button) to the footer, at the bottom, one under another across the
    /// drawer, in the order given.
    pub fn footer(mut self, element: impl IntoElement) -> Self {
        self.footer.push(element.into_any_element());
        self
    }

    /// Runs on Escape, a press on the scrim, and a bottom drawer dragged down far enough
    /// ([`DismissReason::Swiped`]). The host closes it here.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(f));
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

    /// The widest a left or right drawer draws (24 rem by default).
    pub fn max_width(mut self, width: Rems) -> Self {
        self.max_width = width;
        self
    }

    /// The panel's colours: by default the theme's `background`, `foreground` and `border`.
    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// The scrim's colour: by default the theme's `background` at 70%.
    pub fn scrim(mut self, color: impl Into<Hsla>) -> Self {
        self.scrim = Some(color.into());
        self
    }
}

impl ParentElement for Drawer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The content's own style (between the header and the footer): padding, gap, and so on.
impl Styled for Drawer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A drag of a bottom drawer, kept across frames.
#[derive(Default)]
struct Drag {
    /// Where the press started, while one is held.
    from: Option<Pixels>,
    /// How far down it's been dragged.
    offset: Pixels,
    /// The panel's height when last drawn.
    height: Rc<Cell<Pixels>>,
}

impl RenderOnce for Drawer {
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
        let drag = window.use_keyed_state(
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "drag".into()),
            cx,
            |_, _| Drag::default(),
        );
        if opened {
            drag.update(cx, |d, _| {
                d.from = None;
                d.offset = px(0.);
            });
            let initial = self.initial_focus.clone();
            let panel = focus.clone();
            window.defer(cx, move |window, cx| match initial {
                Some(to) => window.focus(&to, cx),
                None => {
                    focus_first_in(&panel, window, cx);
                }
            });
        }
        let scrim = self.open.then(|| self.build(&focus, &drag, cx));
        ModalElement::new(scrim)
    }
}

impl Drawer {
    fn build(
        &mut self,
        focus: &FocusHandle,
        drag: &gpui::Entity<Drag>,
        cx: &App,
    ) -> Stateful<gpui::Div> {
        let theme = cx.theme();
        let colors = self.colors.unwrap_or(PopoverColors {
            background: theme.background,
            foreground: theme.foreground,
            border: theme.border,
        });
        let side = self.side;
        let across = matches!(side, DrawerSide::Bottom | DrawerSide::Top);
        let bottom = side == DrawerSide::Bottom;
        let (offset, height) = {
            let d = drag.read(cx);
            (d.offset, d.height.clone())
        };

        let header = (self.title.is_some() || self.description.is_some()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(css(2.))
                .p(css(16.))
                .min_w_0()
                .when(across, |h| h.items_center().text_center())
                .when_some(self.title.clone(), |h, title| {
                    h.child(
                        div()
                            .debug_selector(|| "gpui-core-drawer-title".into())
                            .text_size(theme.text.base)
                            .font_weight(FontWeight::MEDIUM)
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
        let handle = bottom.then(|| {
            div()
                .flex_none()
                .self_center()
                .mt(css(16.))
                .w(css(100.))
                .h(css(8.))
                .rounded_full()
                .bg(theme.muted)
                .debug_selector(|| "gpui-core-drawer-handle".into())
        });
        // A bottom drawer's grip: its handle and header, which a drag down pulls away.
        let grip = (handle.is_some() || header.is_some()).then(|| {
            let start = drag.clone();
            div()
                .id("drawer-grip")
                .flex()
                .flex_col()
                .flex_none()
                .debug_selector(|| "gpui-core-drawer-grip".into())
                .children(handle)
                .children(header)
                .when(bottom, |g| {
                    g.on_mouse_down(MouseButton::Left, move |e, _, cx| {
                        start.update(cx, |d, _| d.from = Some(e.position.y));
                    })
                })
        });

        let mut body = div().flex().flex_col().min_w_0().px(css(16.));
        gpui::Refineable::refine(body.style(), &self.style);
        let body = body.children(std::mem::take(&mut self.children));
        let footer = std::mem::take(&mut self.footer);
        let footer = (!footer.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(css(8.))
                .p(css(16.))
                .children(footer)
        });
        let scroll = div()
            .id("drawer-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .when(grip.is_none(), |s| s.pt(css(16.)))
            .child(body)
            .children(footer);

        // While a drag is held, the pointer moving anywhere moves the drawer, and letting go
        // dismisses it or puts it back.
        let dragging = drag.read(cx).from.is_some();
        let tracker = {
            let (moved, released) = (drag.clone(), drag.clone());
            let on_swipe = self.on_dismiss.clone();
            let height = height.clone();
            canvas(
                move |bounds, _, _| height.set(bounds.size.height),
                move |_, _, window, _| {
                    if !dragging {
                        return;
                    }
                    let moved = moved.clone();
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        moved.update(cx, |d, cx| {
                            if let Some(from) = d.from {
                                let dy = (e.position.y - from).max(px(0.));
                                let dy = if d.offset == px(0.) && dy < px(SLOP) {
                                    px(0.)
                                } else {
                                    dy
                                };
                                if dy != d.offset {
                                    d.offset = dy;
                                    cx.notify();
                                }
                            }
                        });
                    });
                    let released = released.clone();
                    let on_swipe = on_swipe.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let far = released.update(cx, |d, cx| {
                            let far = d.offset > d.height.get() * DRAWER_DISMISS_FRACTION;
                            d.from = None;
                            d.offset = px(0.);
                            cx.notify();
                            far
                        });
                        if far && let Some(f) = &on_swipe {
                            f(&DismissReason::Swiped, window, cx);
                        }
                    });
                },
            )
            .absolute()
            .size_full()
        };

        let on_escape = self.on_dismiss.clone();
        let mut panel = div()
            .id("drawer")
            .role(Role::Dialog)
            .key_context(POPOVER_CONTEXT)
            .track_focus(focus)
            // Over the scrim, so a press inside isn't one on it.
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .text_size(theme.text.sm)
            .border_color(colors.border)
            .shadow_lg()
            .map(|p| match side {
                DrawerSide::Bottom => p
                    .w_full()
                    .max_h(relative(0.8))
                    .border_t_1()
                    .rounded_t(theme.radius_lg())
                    .top(offset),
                DrawerSide::Top => p
                    .w_full()
                    .max_h(relative(0.8))
                    .border_b_1()
                    .rounded_b(theme.radius_lg()),
                DrawerSide::Left => p
                    .h_full()
                    .w(relative(0.75))
                    .max_w(self.max_width)
                    .border_r_1(),
                DrawerSide::Right => p
                    .h_full()
                    .w(relative(0.75))
                    .max_w(self.max_width)
                    .border_l_1(),
            })
            .debug_selector(|| "gpui-core-drawer".into())
            .on_action(move |_: &Dismiss, window, cx| {
                if let Some(f) = &on_escape {
                    f(&DismissReason::Escape, window, cx);
                }
            })
            .child(tracker)
            .children(grip)
            .child(scroll);
        if let Some(title) = self.title.clone() {
            panel = panel.aria_label(title);
        }
        if let Some(description) = self.description.clone() {
            panel = panel.aria_description(description);
        }

        let on_scrim = self.on_dismiss.clone();
        let (next, prev) = (focus.clone(), focus.clone());
        div()
            .id(self.id.clone())
            .key_context(MODAL_CONTEXT)
            .occlude()
            .flex()
            .map(|s| match side {
                DrawerSide::Bottom => s.flex_col().justify_end(),
                DrawerSide::Top => s.flex_col().justify_start(),
                DrawerSide::Left => s.flex_row().justify_start(),
                DrawerSide::Right => s.flex_row().justify_end(),
            })
            .bg(self.scrim.unwrap_or_else(|| theme.background.opacity(0.7)))
            .debug_selector(|| "gpui-core-drawer-scrim".into())
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
