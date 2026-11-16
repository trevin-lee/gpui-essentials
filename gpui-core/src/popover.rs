//! A popover: a panel that hangs from an element, or from a box or a point in the window, and
//! floats over everything else.
//!
//! The host owns whether it's open: it draws `Popover::new(id).open(open)` every frame and
//! closes it from [`Popover::on_dismiss`], which runs on a click outside it, on Escape, and when
//! what it hangs from scrolls out of view. A press on what it hangs from isn't a click outside,
//! so the host's own toggle there closes it.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, Popover};
//! # struct Panel { open: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Popover::new("more")
//!         .open(self.open)
//!         .on_dismiss(cx.listener(|this, _, _, cx| {
//!             this.open = false;
//!             cx.notify();
//!         }))
//!         .trigger(Button::new("more-button").label("More").on_click(cx.listener(
//!             |this, _, _, cx| {
//!                 this.open = !this.open;
//!                 cx.notify();
//!             },
//!         )))
//!         .child("Anything")
//! }
//! # }
//! ```

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, FocusHandle, GlobalElementId, Hsla,
    InspectorElementId, InteractiveElement as _, IntoElement, KeyBinding, LayoutId, ParentElement,
    Pixels, Point, Position, Rems, RenderOnce, Role, Stateful, StatefulInteractiveElement as _,
    Style, StyleRefinement, Styled, Window, actions, div, point, px, rems,
};

use crate::theme::{ActiveTheme as _, Theme};

actions!(gpui_core_popover, [Dismiss]);

pub(crate) const CONTEXT: &str = "GpuiCorePopover";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some(CONTEXT))]);
}

/// How far a popover keeps from the window's edges, and from what it hangs from, in CSS pixels.
const EDGE: f32 = 8.;
const GAP: f32 = 4.;

/// Above other deferred drawing such as tooltips' hosts; the same as the crate's other overlays.
pub(crate) const PRIORITY: usize = 1;

pub(crate) type DismissFn = Rc<dyn Fn(&DismissReason, &mut Window, &mut App)>;

/// Why a popover asks to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DismissReason {
    /// A press outside it (and outside what it hangs from).
    ClickOutside,
    Escape,
    /// What it hangs from scrolled out of view.
    ScrolledAway,
}

/// A popover's colours. By default they're the theme's `popover`, `popover-foreground` and
/// `border`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub border: Hsla,
}

impl PopoverColors {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.popover,
            foreground: theme.popover_foreground,
            border: theme.border,
        }
    }
}

/// What a popover hangs from, in window coordinates: a box, or a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverAnchor(Bounds<Pixels>);

impl From<Bounds<Pixels>> for PopoverAnchor {
    fn from(b: Bounds<Pixels>) -> Self {
        Self(b)
    }
}

impl From<Point<Pixels>> for PopoverAnchor {
    fn from(p: Point<Pixels>) -> Self {
        Self(Bounds::new(p, gpui::size(px(0.), px(0.))))
    }
}

/// A panel below what it hangs from (above it where there's more room there), at least as wide
/// as that and at most as wide as the window, as tall as its content up to a maximum and then
/// scrolling. It's drawn only while open.
///
/// While open it has the keyboard (unless [`Popover::take_focus`] is off); when it closes, the
/// focus goes back to what had it when it opened, or to [`Popover::return_focus`]. For that, keep
/// drawing the popover while it's closed: closed, it draws nothing but its trigger.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    open: bool,
    trigger: Option<AnyElement>,
    anchor: Option<PopoverAnchor>,
    on_dismiss: Option<DismissFn>,
    take_focus: bool,
    return_focus: Option<FocusHandle>,
    max_height: Rems,
    match_trigger_width: bool,
    close_when_scrolled_away: bool,
    colors: Option<PopoverColors>,
    role: Role,
    label: Option<gpui::SharedString>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            trigger: None,
            anchor: None,
            on_dismiss: None,
            take_focus: true,
            return_focus: None,
            max_height: rems(24.),
            match_trigger_width: true,
            close_when_scrolled_away: true,
            colors: None,
            role: Role::Dialog,
            label: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// An element drawn in place, which the popover hangs from. When it scrolls out of view,
    /// the popover asks to close.
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// A box or a point in window coordinates to hang from, for a popover with no
    /// [`trigger`](Self::trigger) (a painted element, the pointer). A trigger wins over it.
    pub fn anchor(mut self, anchor: impl Into<PopoverAnchor>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    /// Runs on a click outside the popover, on Escape, and when its trigger scrolls out of view.
    /// The host closes it here.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    /// Whether it takes the keyboard while open (on by default). Off, the focus stays where it
    /// was, and Escape is the host's to handle.
    pub fn take_focus(mut self, take: bool) -> Self {
        self.take_focus = take;
        self
    }

    /// Where the focus goes when it closes, instead of what had it when it opened.
    pub fn return_focus(mut self, handle: &FocusHandle) -> Self {
        self.return_focus = Some(handle.clone());
        self
    }

    /// The tallest it draws before its content scrolls (24 rem by default). It's never taller
    /// than the room the window has.
    pub fn max_height(mut self, height: Rems) -> Self {
        self.max_height = height;
        self
    }

    /// Whether it's at least as wide as what it hangs from (on by default). Off, it's as wide as
    /// its content: a month under a wide field.
    pub fn match_trigger_width(mut self, matching: bool) -> Self {
        self.match_trigger_width = matching;
        self
    }

    /// Whether it asks to close when what it hangs from scrolls out of view (on by default).
    /// Off, it stays open at the edge of the view nearest that, until what it hangs from comes
    /// back: for a host that scrolls a table's cell under an open menu.
    pub fn close_when_scrolled_away(mut self, close: bool) -> Self {
        self.close_when_scrolled_away = close;
        self
    }

    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Its role for assistive tech: a dialog by default; a menu or a list box for those.
    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for Popover {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The panel's own style: padding, width, and so on. It applies to the panel, not the trigger.
impl Styled for Popover {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What an overlay (a popover, a sheet) remembers between frames.
struct OverlayState {
    focus: FocusHandle,
    open: bool,
    /// What had the focus when it opened.
    opener: Option<FocusHandle>,
}

/// An overlay's focus, kept across frames under `id`: on opening it takes the keyboard (with
/// `take`) and remembers what had it; on closing it gives it back to `return_to` or to that,
/// if the focus is still inside it or lost.
pub(crate) fn overlay_focus(
    id: &ElementId,
    open: bool,
    take: bool,
    return_to: Option<FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    let state = window.use_keyed_state(id.clone(), cx, |_, cx| OverlayState {
        focus: cx.focus_handle(),
        open: false,
        opener: None,
    });
    let (was_open, focus) = {
        let s = state.read(cx);
        (s.open, s.focus.clone())
    };
    if open && !was_open {
        let opener = window.focused(cx);
        state.update(cx, |s, _| {
            s.open = true;
            s.opener = opener;
        });
        if take {
            let focus = focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
    } else if !open && was_open {
        let opener = state.update(cx, |s, _| {
            s.open = false;
            s.opener.take()
        });
        // Only if the focus is still inside it, or lost: a click outside that focused
        // something keeps that.
        let inside = focus.contains_focused(window, cx) || window.focused(cx).is_none();
        if let Some(to) = return_to.or(opener).filter(|_| inside) {
            window.defer(cx, move |window, cx| window.focus(&to, cx));
        }
    }
    focus
}

impl RenderOnce for Popover {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let focus = overlay_focus(
            &self.id,
            self.open,
            self.take_focus,
            self.return_focus.clone(),
            window,
            cx,
        );

        let anchor = Rc::new(Cell::new(self.anchor.map(|a| a.0)));
        let panel = self.open.then(|| self.panel(&focus, anchor.clone(), cx));
        PopoverElement {
            trigger: self.trigger,
            anchor,
            panel,
            max_height: self.max_height,
            match_trigger_width: self.match_trigger_width,
            close_when_scrolled_away: self.close_when_scrolled_away,
            on_dismiss: self.on_dismiss,
        }
    }
}

impl Popover {
    fn panel(
        &mut self,
        focus: &FocusHandle,
        anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
        cx: &App,
    ) -> Stateful<gpui::Div> {
        let theme = cx.theme();
        let colors = self
            .colors
            .unwrap_or_else(|| PopoverColors::from_theme(theme));
        let on_dismiss = self.on_dismiss.clone();
        let on_escape = self.on_dismiss.clone();
        let mut scroll = div()
            .id("popover-content")
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll();
        gpui::Refineable::refine(scroll.style(), &self.style);
        let scroll = scroll.children(std::mem::take(&mut self.children));
        let mut panel = div()
            .id(self.id.clone())
            .role(self.role)
            .key_context(CONTEXT)
            .track_focus(focus)
            .occlude()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(theme.radius_md())
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_color(colors.foreground)
            .shadow_md()
            .debug_selector(|| "gpui-core-popover".into())
            .on_action(move |_: &Dismiss, window, cx| {
                if let Some(f) = &on_escape {
                    f(&DismissReason::Escape, window, cx);
                }
            })
            .on_mouse_down_out(move |e, window, cx| {
                // A press on what it hangs from is the host's toggle, not a click outside.
                if anchor.get().is_some_and(|a| a.contains(&e.position)) {
                    return;
                }
                if let Some(f) = &on_dismiss {
                    f(&DismissReason::ClickOutside, window, cx);
                }
            })
            .child(scroll);
        if let Some(label) = self.label.clone() {
            panel = panel.aria_label(label);
        }
        panel
    }
}

/// Where a panel of `size` goes: below `anchor`, or above it where it doesn't fit below and
/// there's more room above; from the anchor's left, moved in to keep `edge` from the window's
/// sides.
fn place(
    anchor: Bounds<Pixels>,
    size: gpui::Size<Pixels>,
    view: Bounds<Pixels>,
    edge: Pixels,
    gap: Pixels,
) -> Point<Pixels> {
    let below = view.bottom() - anchor.bottom() - gap - edge;
    let above = anchor.top() - view.top() - gap - edge;
    let y = if size.height <= below || below >= above {
        anchor.bottom() + gap
    } else {
        anchor.top() - gap - size.height
    };
    let x = anchor
        .left()
        .min(view.right() - edge - size.width)
        .max(view.left() + edge.min(anchor.left()));
    point(x.round(), y.round())
}

/// `b` moved the least way that puts it inside `inside` (its own size kept, up to that's).
fn hold_inside(b: Bounds<Pixels>, inside: Bounds<Pixels>) -> Bounds<Pixels> {
    let size = gpui::size(
        b.size.width.min(inside.size.width),
        b.size.height.min(inside.size.height),
    );
    let x = b.left().max(inside.left()).min(inside.right() - size.width);
    let y = b.top().max(inside.top()).min(inside.bottom() - size.height);
    Bounds::new(point(x, y), size)
}

/// Draws the trigger in place, and the open panel over everything, placed from where the
/// trigger (or the anchor) is this frame.
struct PopoverElement {
    trigger: Option<AnyElement>,
    anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
    panel: Option<Stateful<gpui::Div>>,
    max_height: Rems,
    match_trigger_width: bool,
    close_when_scrolled_away: bool,
    on_dismiss: Option<DismissFn>,
}

impl IntoElement for PopoverElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PopoverElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let layout = match &mut self.trigger {
            Some(trigger) => trigger.request_layout(window, cx),
            // Nothing in the flow: an anchored popover takes no room where it's declared.
            None => window.request_layout(
                Style {
                    position: Position::Absolute,
                    ..Style::default()
                },
                None,
                cx,
            ),
        };
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(trigger) = &mut self.trigger {
            trigger.prepaint(window, cx);
            self.anchor.set(Some(bounds));
        }
        let Some(panel) = self.panel.take() else {
            return;
        };
        let Some(mut anchor) = self.anchor.get() else {
            return;
        };
        let view = Bounds::new(Point::default(), window.viewport_size());
        let visible = if self.trigger.is_some() {
            window.content_mask().bounds
        } else {
            view
        };
        // Scrolled out of view (a point anchor counts when it's on the window).
        let shown = anchor.intersects(&visible)
            || (anchor.size.width == px(0.) && visible.contains(&anchor.origin));
        if !shown && self.close_when_scrolled_away {
            if let Some(f) = self.on_dismiss.clone() {
                window.defer(cx, move |window, cx| {
                    f(&DismissReason::ScrolledAway, window, cx)
                });
            }
            return;
        }
        if !shown {
            // Held at the edge of the view nearest where it hangs from.
            anchor = hold_inside(anchor, visible.intersect(&view));
        }

        let rem = window.rem_size();
        let (edge, gap) = (rem * (EDGE / 15.), rem * (GAP / 15.));
        let below = view.bottom() - anchor.bottom() - gap - edge;
        let above = anchor.top() - view.top() - gap - edge;
        let room = below.max(above).max(px(0.));
        let height = self.max_height.to_pixels(rem).min(room);
        let widest = (view.size.width - edge * 2.).max(px(0.));
        let narrowest = if self.match_trigger_width {
            anchor.size.width.min(widest)
        } else {
            px(0.)
        };
        let mut panel = panel
            .max_h(height)
            .max_w(widest)
            .min_w(narrowest)
            .into_any_element();
        // Laid out on its own, now that what it hangs from has been placed this frame.
        let size = panel.layout_as_root(
            gpui::size(
                gpui::AvailableSpace::MaxContent,
                gpui::AvailableSpace::MaxContent,
            ),
            window,
            cx,
        );
        let origin = place(anchor, size, view, edge, gap);
        window.defer_draw(panel, origin, PRIORITY, None::<ContentMask<Pixels>>);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(trigger) = &mut self.trigger {
            trigger.paint(window, cx);
        }
    }
}
