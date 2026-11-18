//! The provider: react-dnd's `DndProvider`. It follows the drag and draws its preview.

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, DispatchPhase, Element, ElementId, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Size, Style,
    Styled as _, Window, div, rems,
};
use gpui_core::ActiveTheme as _;

use crate::item::DragItem;
use crate::source::{Backend, PreviewSpec};
use crate::state;

/// The preview's place among deferred draws: above gpui-core's popovers (1), dialogs (2) and
/// toasts (3), and anything else drawn in the window.
pub const PREVIEW_PRIORITY: usize = 100;

/// Follows the drag and draws its preview above everything (react-dnd's `DndProvider`). Wrap
/// the window's content in one; with gpui-core, inside `Root`, as the view's content:
///
/// ```ignore
/// impl Render for Board {
///     fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
///         DndProvider::new(div().size_full().child(self.columns()))
///     }
/// }
/// // Root::new(cx.new(|_| Board::new()), window, cx)
/// ```
///
/// Only the outermost provider in a window does anything; one inside it just draws its child.
/// So a crate can wrap its own view in one and still work in a host that wraps the whole
/// window. Without a provider, sources take presses but no drag ever begins.
pub struct DndProvider {
    child: Option<AnyElement>,
    mouse: bool,
}

impl DndProvider {
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            child: Some(child.into_any_element()),
            mouse: true,
        }
    }

    /// Whether the mouse and trackpad start drags. On unless turned off.
    pub fn mouse(mut self, on: bool) -> Self {
        self.mouse = on;
        self
    }
}

impl IntoElement for DndProvider {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for DndProvider {
    type RequestLayoutState = AnyElement;
    /// Whether it's the outermost provider, the one that works.
    type PrepaintState = bool;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut child = self.child.take().expect("laid out once");
        (child.request_layout(window, cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let shared = state::shared(cx);
        let outermost = {
            let mut state = shared.borrow_mut();
            state.provider_depth += 1;
            let outermost = state.provider_depth == 1;
            if outermost {
                state
                    .provider_views
                    .insert(state::window_id(window), window.current_view());
            }
            outermost
        };
        child.prepaint(window, cx);
        shared.borrow_mut().provider_depth -= 1;
        if outermost {
            draw_preview(window, cx);
        }
        outermost
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        outermost: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
        if !*outermost {
            return;
        }
        let window_id = state::window_id(window);
        if state::snapshot(cx).is_some() {
            window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
        }
        if self.mouse {
            listen_to_mouse(window);
        } else {
            // Sources still take presses; a press that can't become a drag is forgotten.
            let shared = state::shared(cx);
            let mut state = shared.borrow_mut();
            if state
                .pending
                .as_ref()
                .is_some_and(|p| p.window == window_id)
            {
                state.pending = None;
            }
        }
    }
}

fn listen_to_mouse(window: &mut Window) {
    // A new press forgets an old one, before the sources under it hear it.
    window.on_mouse_event(|_: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture {
            let shared = state::shared(cx);
            let mut state = shared.borrow_mut();
            if state
                .pending
                .as_ref()
                .is_some_and(|p| p.window == state::window_id(window))
            {
                state.pending = None;
            }
        }
    });
    window.on_mouse_event(|event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        let window_id = state::window_id(window);
        let shared = state::shared(cx);
        let dragging = shared
            .borrow()
            .drag
            .as_ref()
            .is_some_and(|d| d.window == window_id && d.snap.backend == Backend::Mouse);
        if dragging {
            if event.pressed_button == Some(MouseButton::Left) {
                state::move_to(event.position, event.modifiers, window, cx);
            } else {
                // The button came up where the window didn't see it.
                state::release(event.position, window, cx);
            }
            return;
        }
        let start = {
            let mut state = shared.borrow_mut();
            let Some(pending) = state.pending.as_ref().filter(|p| p.window == window_id) else {
                return;
            };
            if event.pressed_button != Some(MouseButton::Left) {
                state.pending = None;
                return;
            }
            let moved = event.position - pending.at;
            let distance = f32::from(moved.x).hypot(f32::from(moved.y));
            if distance <= f32::from(pending.threshold) {
                return;
            }
            state.pending.take()
        };
        if let Some(pending) = start
            && state::begin(&pending.source, pending.at, Backend::Mouse, window, cx)
        {
            state::move_to(event.position, event.modifiers, window, cx);
        }
    });
    window.on_mouse_event(|event: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
            return;
        }
        let window_id = state::window_id(window);
        let shared = state::shared(cx);
        let mouse_drag = {
            let mut state = shared.borrow_mut();
            state.pending = None;
            state
                .drag
                .as_ref()
                .is_some_and(|d| d.window == window_id && d.snap.backend == Backend::Mouse)
        };
        // The release ends the drag and isn't a click on whatever it ends over.
        if mouse_drag && state::release(event.position, window, cx) {
            cx.stop_propagation();
        }
    });
}

/// Draws the drag's preview, if it has one in this window, above everything else.
fn draw_preview(window: &mut Window, cx: &mut App) {
    let Some(snap) = state::snapshot(cx) else {
        return;
    };
    let shared = state::shared(cx);
    let build = {
        let state = shared.borrow();
        let Some(drag) = state.drag.as_ref() else {
            return;
        };
        if drag.window != state::window_id(window) {
            return;
        }
        match &drag.spec.preview {
            PreviewSpec::Default(build) => build.clone(),
            PreviewSpec::None => return,
        }
    };
    let preview = match build {
        Some(build) => build(&snap.item, window, cx),
        None => default_preview(&snap.item, snap.source_size, cx),
    };
    let origin = snap.current - snap.grab();
    let preview = div()
        .debug_selector(|| "dnd-preview".into())
        .child(preview)
        .into_any_element();
    let mut floating = Floating {
        child: preview,
        origin,
    }
    .into_any_element();
    floating.request_layout(window, cx);
    window.defer_draw(floating, origin, PREVIEW_PRIORITY, None);
}

/// An element laid out on its own at a place in the window, as GPUI lays out its own drag's
/// view: deferred draws are only prepainted and painted, so this one's own layout is an empty
/// leaf, and its child is laid out as a root when it's prepainted.
struct Floating {
    child: AnyElement,
    origin: Point<Pixels>,
}

impl IntoElement for Floating {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Floating {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (window.request_layout(Style::default(), None, cx), ())
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
        self.child
            .prepaint_as_root(self.origin, AvailableSpace::min_size(), window, cx);
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

/// A card of the source's size, lifted: the theme's card colours, its ring and a shadow, and
/// the count when several items are dragged.
fn default_preview(item: &DragItem, size: Size<Pixels>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let card = div()
        .w(size.width)
        .h(size.height)
        .rounded(theme.radius_lg())
        .bg(theme.card)
        .border_1()
        .border_color(theme.ring)
        .shadow_lg()
        .opacity(0.9);
    if item.count() < 2 {
        return card.into_any_element();
    }
    card.child(
        div()
            .debug_selector(|| "dnd-preview-count".into())
            .absolute()
            .top(rems(-0.5))
            .right(rems(-0.5))
            .min_w(rems(1.25))
            .h(rems(1.25))
            .px(rems(0.375))
            .rounded_full()
            .bg(theme.primary)
            .text_color(theme.primary_foreground)
            .text_xs()
            .flex()
            .items_center()
            .justify_center()
            .child(item.count().to_string()),
    )
    .into_any_element()
}
