//! A popover and a bottom sheet, drawn by the calendar for now.
//!
//! These are stand-ins for gpui-core's coming `Popover` and `Sheet`, shaped like their planned
//! API (`new(id).anchor(..).open(..).on_dismiss(..).child(..)`) so that swapping to them later is
//! mechanical. Both float over everything else, dismiss on a click outside them (the calendar
//! dismisses them on Escape, as it keeps the keyboard), and never take focus: it stays with the
//! calendar that opened them.

use std::rc::Rc;

use gpui::*;
use gpui_core::Theme;

type Dismiss = Rc<dyn Fn(&mut Window, &mut App)>;

/// A small panel below a point or a box, moved above it (or along the edge) where the window
/// has no room below.
#[derive(IntoElement)]
pub(crate) struct Popover {
    id: ElementId,
    anchor: Bounds<Pixels>,
    /// About how tall it draws, in CSS pixels: what decides whether it fits below or above.
    height: f32,
    open: bool,
    on_dismiss: Option<Dismiss>,
    children: Vec<AnyElement>,
}

impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            anchor: Bounds::default(),
            height: 160.,
            open: true,
            on_dismiss: None,
            children: Vec::new(),
        }
    }

    /// What it hangs from, in window coordinates: a box, or a point.
    pub fn anchor(mut self, anchor: impl Into<OverlayAnchor>) -> Self {
        self.anchor = anchor.into().0;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// About how tall its content draws, in CSS pixels, so it can choose a side that fits.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

/// The widest a popover draws, in CSS pixels.
const MAX_WIDTH: f32 = 300.;

/// Where a popover hangs from.
pub(crate) struct OverlayAnchor(Bounds<Pixels>);

impl From<Bounds<Pixels>> for OverlayAnchor {
    fn from(b: Bounds<Pixels>) -> Self {
        Self(b)
    }
}

impl From<Point<Pixels>> for OverlayAnchor {
    fn from(p: Point<Pixels>) -> Self {
        Self(Bounds::new(p, size(px(0.), px(0.))))
    }
}

impl RenderOnce for Popover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.open {
            return Empty.into_any_element();
        }
        let theme = Theme::global(cx);
        let c = theme.colors;
        let radius = theme.radius_lg();
        let dismiss = self.on_dismiss.clone();
        // Below the anchor; above it where there's no room below; beside it (right, else left)
        // where there's room on neither. It never covers what it hangs from.
        let view = window.viewport_size();
        let rem = window.rem_size();
        let (gap, edge) = (px(6.), px(8.));
        let tall = rem * (self.height / 15.);
        let wide = rem * (MAX_WIDTH / 15.);
        let a = self.anchor;
        let (corner, own, offset) = if view.height - a.bottom() - gap - edge >= tall {
            (Anchor::BottomLeft, Anchor::TopLeft, point(px(0.), gap))
        } else if a.top() - gap - edge >= tall {
            (Anchor::TopLeft, Anchor::BottomLeft, point(px(0.), -gap))
        } else if view.width - a.right() - gap - edge >= wide {
            (Anchor::TopRight, Anchor::TopLeft, point(gap, px(0.)))
        } else {
            (Anchor::TopLeft, Anchor::TopRight, point(-gap, px(0.)))
        };
        deferred(
            anchored()
                .attach(a, corner)
                .anchor(own)
                .offset(offset)
                .snap_to_window_with_margin(edge)
                .child(
                    div()
                        .id(self.id)
                        .occlude()
                        .flex()
                        .flex_col()
                        .min_w(rems(14.))
                        .max_w(rems(MAX_WIDTH / 15.))
                        .p(rems(4. / 15.))
                        .bg(c.popover)
                        .text_color(c.popover_foreground)
                        .border_1()
                        .border_color(c.border)
                        .rounded(radius)
                        .shadow_lg()
                        .debug_selector(|| "calendar-popover".into())
                        .on_mouse_down_out(move |_, window, cx| {
                            if let Some(f) = &dismiss {
                                f(window, cx);
                            }
                        })
                        .children(self.children),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }
}

/// A panel across the bottom of the calendar, over a scrim that dismisses it, for touch.
#[derive(IntoElement)]
pub(crate) struct Sheet {
    id: ElementId,
    open: bool,
    on_dismiss: Option<Dismiss>,
    children: Vec<AnyElement>,
}

impl Sheet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: true,
            on_dismiss: None,
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl RenderOnce for Sheet {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.open {
            return Empty.into_any_element();
        }
        let theme = Theme::global(cx);
        let c = theme.colors;
        let radius = theme.radius_xl();
        let dismiss = self.on_dismiss.clone();
        deferred(
            div()
                .id(self.id)
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .justify_end()
                .bg(hsla(0., 0., 0., 0.45))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    if let Some(f) = &dismiss {
                        f(window, cx);
                    }
                })
                .child(
                    div()
                        .id("sheet")
                        .occlude()
                        .flex()
                        .flex_col()
                        .w_full()
                        .px(rems(12. / 15.))
                        .pt(rems(8. / 15.))
                        .pb(rems(20. / 15.))
                        .bg(c.popover)
                        .text_color(c.popover_foreground)
                        .border_t_1()
                        .border_color(c.border)
                        .rounded_t(radius)
                        .debug_selector(|| "calendar-sheet".into())
                        // A press inside the sheet isn't one on the scrim.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            // The grabber.
                            div()
                                .self_center()
                                .w(rems(36. / 15.))
                                .h(rems(4. / 15.))
                                .mb(rems(8. / 15.))
                                .rounded_full()
                                .bg(c.muted_foreground.opacity(0.5)),
                        )
                        .children(self.children),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }
}
