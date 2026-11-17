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
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, FocusHandle, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, InteractiveElement as _, IntoElement, KeyBinding,
    LayoutId, ParentElement, Pixels, Point, Position, Rems, RenderOnce, Role, Stateful,
    StatefulInteractiveElement as _, Style, StyleRefinement, Styled, Window, WindowId, actions,
    div, point, px, rems,
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

/// A modal's (a dialog's): over popovers. What's drawn from inside a modal goes in its layer
/// (see [`layer`]), so a menu in a dialog is over the dialog.
pub(crate) const MODAL_PRIORITY: usize = PRIORITY + 1;

/// The layer of the overlay whose content is being placed.
struct Layer(usize);

impl gpui::Global for Layer {}

/// The priority an overlay drawn now is deferred at: [`PRIORITY`], or the layer of the overlay
/// it's drawn from inside if that's higher (a popover in a dialog).
pub(crate) fn layer(cx: &App) -> usize {
    cx.try_global::<Layer>()
        .map_or(PRIORITY, |l| l.0.max(PRIORITY))
}

/// Draws `child`, an overlay deferred at `priority`, with that as the [`layer`] of the overlays
/// drawn from inside it.
pub(crate) struct InLayer {
    priority: usize,
    child: AnyElement,
}

impl InLayer {
    pub(crate) fn new(priority: usize, child: AnyElement) -> Self {
        Self { priority, child }
    }
}

impl IntoElement for InLayer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InLayer {
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
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let before = cx.try_global::<Layer>().map(|l| l.0);
        cx.set_global(Layer(self.priority));
        self.child.prepaint(window, cx);
        match before {
            Some(p) => cx.set_global(Layer(p)),
            None => {
                cx.remove_global::<Layer>();
            }
        }
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
        self.child.paint(window, cx);
    }
}

pub(crate) type DismissFn = Rc<dyn Fn(&DismissReason, &mut Window, &mut App)>;

/// Why a popover asks to close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DismissReason {
    /// A press outside it (and outside what it hangs from).
    ClickOutside,
    Escape,
    /// What it hangs from scrolled out of view.
    ScrolledAway,
    /// Its close button (a dialog's ×).
    CloseButton,
    /// Dragged away (a drawer pulled down past its threshold).
    Swiped,
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

/// The colours of the panel being laid out: a submenu drawn from inside it takes them.
struct PanelColors(PopoverColors);

impl gpui::Global for PanelColors {}

/// Runs `f` (a panel's layout) with `colors` as the panel's, for what's drawn from inside it.
pub(crate) fn with_panel_colors<R>(
    colors: PopoverColors,
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    let before = cx.try_global::<PanelColors>().map(|g| g.0);
    cx.set_global(PanelColors(colors));
    let out = f(window, cx);
    match before {
        Some(c) => cx.set_global(PanelColors(c)),
        None => {
            cx.remove_global::<PanelColors>();
        }
    }
    out
}

/// The colours of the panel being laid out, or the theme's outside one.
pub(crate) fn panel_colors(cx: &App) -> PopoverColors {
    cx.try_global::<PanelColors>()
        .map(|g| g.0)
        .unwrap_or_else(|| PopoverColors::from_theme(cx.theme()))
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

/// Which side of what it hangs from a popover is drawn on. It goes to the opposite side where
/// it doesn't fit and there's more room there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PopoverSide {
    Top,
    Right,
    #[default]
    Bottom,
    Left,
}

/// Where a popover lines up along the side it's on: its start with the start of what it hangs
/// from (its left, or its top beside it), centred on it, or its end with the end. It's moved in
/// to stay in the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PopoverAlign {
    #[default]
    Start,
    Center,
    End,
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
    side: PopoverSide,
    align: PopoverAlign,
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
            side: PopoverSide::Bottom,
            align: PopoverAlign::Start,
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

    /// Which side of what it hangs from it's drawn on (below unless given).
    pub fn side(mut self, side: PopoverSide) -> Self {
        self.side = side;
        self
    }

    /// Where it lines up along that side (its start with the start, unless given).
    pub fn align(mut self, align: PopoverAlign) -> Self {
        self.align = align;
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
        let colors = self
            .colors
            .unwrap_or_else(|| PopoverColors::from_theme(cx.theme()));
        let panel = self.open.then(|| self.panel(&focus, anchor.clone(), cx));
        PopoverElement {
            colors,
            trigger: self.trigger,
            anchor,
            panel,
            max_height: self.max_height,
            match_trigger_width: self.match_trigger_width,
            close_when_scrolled_away: self.close_when_scrolled_away,
            side: self.side,
            align: self.align,
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
                // A press on a submenu drawn beside the panel isn't outside it.
                if claimed(e.position, window, cx) {
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

/// The areas drawn from inside a popover but outside its panel (submenus), by window and
/// element: a press on one isn't a press outside the popover. Each is a hitbox from the frame
/// that drew it, so one no longer drawn is never hit.
///
/// Each window keeps the [`MAX_CLAIMS`] it was drawn with last: one drawn claims again every
/// time it's drawn, so those in view are always the latest, and the claims of menus long gone
/// (a menu per row, opened once each) don't pile up.
#[derive(Default)]
struct Claims(HashMap<WindowId, Vec<(ElementId, Hitbox)>>);

impl gpui::Global for Claims {}

/// More than are ever drawn at once (a submenu's submenus, a few deep).
const MAX_CLAIMS: usize = 32;

impl Claims {
    fn claim(&mut self, window: WindowId, key: ElementId, hitbox: Hitbox, open: &[WindowId]) {
        // A window that's closed has nothing to claim.
        self.0.retain(|w, _| open.contains(w));
        let list = self.0.entry(window).or_default();
        list.retain(|(k, _)| *k != key);
        list.push((key, hitbox));
        if list.len() > MAX_CLAIMS {
            list.remove(0);
        }
    }

    fn count(&self) -> usize {
        self.0.values().map(Vec::len).sum()
    }
}

fn claimed(at: Point<Pixels>, window: &Window, cx: &App) -> bool {
    let id = window.window_handle().window_id();
    cx.try_global::<Claims>().is_some_and(|c| {
        c.0.get(&id)
            .is_some_and(|list| list.iter().any(|(_, h)| h.is_hovered_at(at, window)))
    })
}

/// How many presses are claimed across every window, for tests.
#[doc(hidden)]
pub fn claim_count(cx: &App) -> usize {
    cx.try_global::<Claims>().map_or(0, Claims::count)
}

/// Draws `child` and claims the area it covers for the popover it was drawn from inside, so a
/// press on it isn't taken for a press outside that popover.
pub(crate) struct ClaimArea {
    key: ElementId,
    child: AnyElement,
}

impl ClaimArea {
    pub(crate) fn new(key: ElementId, child: AnyElement) -> Self {
        Self { key, child }
    }
}

impl IntoElement for ClaimArea {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ClaimArea {
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
        (self.child.request_layout(window, cx), ())
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
        self.child.prepaint(window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let id = window.window_handle().window_id();
        let open: Vec<WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
        cx.default_global::<Claims>()
            .claim(id, self.key.clone(), hitbox, &open);
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
        self.child.paint(window, cx);
    }
}

/// Where a panel of `size` goes: on `side` of `anchor`, or the opposite side where it doesn't
/// fit there and there's more room on that one; lined up along it by `align`, moved in to keep
/// `edge` from the window's sides. Below and from the anchor's left unless told otherwise.
fn place(
    anchor: Bounds<Pixels>,
    size: gpui::Size<Pixels>,
    view: Bounds<Pixels>,
    edge: Pixels,
    gap: Pixels,
    side: PopoverSide,
    align: PopoverAlign,
) -> Point<Pixels> {
    // Along the side: from `start` to `end` of the anchor, a panel `len` long, in `lo..hi`.
    let along = |start: Pixels, end: Pixels, len: Pixels, lo: Pixels, hi: Pixels| {
        let want = match align {
            PopoverAlign::Start => start,
            PopoverAlign::Center => (start + end - len) / 2.,
            PopoverAlign::End => end - len,
        };
        want.min(hi - edge - len).max(lo + edge.min(start))
    };
    match side {
        PopoverSide::Top | PopoverSide::Bottom => {
            let below = view.bottom() - anchor.bottom() - gap - edge;
            let above = anchor.top() - view.top() - gap - edge;
            let down = if side == PopoverSide::Bottom {
                size.height <= below || below >= above
            } else {
                !(size.height <= above || above >= below)
            };
            let y = if down {
                anchor.bottom() + gap
            } else {
                anchor.top() - gap - size.height
            };
            let x = along(
                anchor.left(),
                anchor.right(),
                size.width,
                view.left(),
                view.right(),
            );
            point(x.round(), y.round())
        }
        PopoverSide::Left | PopoverSide::Right => {
            let after = view.right() - anchor.right() - gap - edge;
            let before = anchor.left() - view.left() - gap - edge;
            let right = if side == PopoverSide::Right {
                size.width <= after || after >= before
            } else {
                !(size.width <= before || before >= after)
            };
            let x = if right {
                anchor.right() + gap
            } else {
                anchor.left() - gap - size.width
            };
            let y = along(
                anchor.top(),
                anchor.bottom(),
                size.height,
                view.top(),
                view.bottom(),
            );
            point(x.round(), y.round())
        }
    }
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
    /// The panel's colours, which a submenu inside it takes.
    colors: PopoverColors,
    trigger: Option<AnyElement>,
    anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
    panel: Option<Stateful<gpui::Div>>,
    max_height: Rems,
    match_trigger_width: bool,
    close_when_scrolled_away: bool,
    side: PopoverSide,
    align: PopoverAlign,
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
        let seen = visible.intersect(&view);
        if !shown {
            // Held at the edge of the view nearest where it hangs from.
            anchor = hold_inside(anchor, seen);
        } else if anchor.size.width > px(0.) {
            // Partly scrolled off (a table's cell half under its frozen column): hung from the
            // part that's seen, so it never starts off the view.
            let part = anchor.intersect(&seen);
            if part.size.width > px(0.) && part.size.height > px(0.) {
                anchor = part;
            }
        }

        let rem = window.rem_size();
        let (edge, gap) = (rem * (EDGE / 15.), rem * (GAP / 15.));
        let room = match self.side {
            PopoverSide::Top | PopoverSide::Bottom => {
                let below = view.bottom() - anchor.bottom() - gap - edge;
                let above = anchor.top() - view.top() - gap - edge;
                below.max(above)
            }
            // Beside it: the window's height.
            PopoverSide::Left | PopoverSide::Right => view.size.height - edge * 2.,
        }
        .max(px(0.));
        let height = self.max_height.to_pixels(rem).min(room);
        let widest = (view.size.width - edge * 2.).max(px(0.));
        let narrowest = if self.match_trigger_width {
            anchor.size.width.min(widest)
        } else {
            px(0.)
        };
        let layer = layer(cx);
        let panel = panel
            .max_h(height)
            .max_w(widest)
            .min_w(narrowest)
            .into_any_element();
        let mut panel = InLayer::new(layer, panel).into_any_element();
        // Laid out on its own, now that what it hangs from has been placed this frame.
        let size = with_panel_colors(self.colors, window, cx, |window, cx| {
            panel.layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::MaxContent,
                    gpui::AvailableSpace::MaxContent,
                ),
                window,
                cx,
            )
        });
        let origin = place(anchor, size, view, edge, gap, self.side, self.align);
        window.defer_draw(panel, origin, layer, None::<ContentMask<Pixels>>);
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
