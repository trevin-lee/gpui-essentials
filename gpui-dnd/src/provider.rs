//! The provider: react-dnd's `DndProvider`. It follows the drag and draws its preview.

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, DispatchPhase, Element, ElementId, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Size, Style,
    Styled as _, Window, div, rems,
};
use gpui_core::ActiveTheme as _;

use std::rc::Rc;

use crate::item::DragItem;
use crate::keyboard::{self, Announcements};
use crate::monitor::{DragLayerMonitor, DragMonitor};
use crate::source::{Backend, PreviewSpec};
use crate::state;

type Layer = Rc<dyn Fn(&DragLayerMonitor, &mut Window, &mut App) -> Option<AnyElement>>;

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
    keyboard: bool,
    touch: bool,
    announcements: Option<Announcements>,
    layer: Option<Layer>,
}

impl DndProvider {
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            child: Some(child.into_any_element()),
            mouse: true,
            keyboard: true,
            touch: true,
            announcements: None,
            layer: None,
        }
    }

    /// Whether touches pick up sources (a long press, or a move on an at-once source). On
    /// unless turned off.
    pub fn touch(mut self, on: bool) -> Self {
        self.touch = on;
        self
    }

    /// Whether the keyboard picks up sources (Space or Enter on a focused source). On unless
    /// turned off.
    pub fn keyboard(mut self, on: bool) -> Self {
        self.keyboard = on;
        self
    }

    /// What the live region says during a keyboard drag, in place of the English defaults.
    pub fn announcements(mut self, announcements: Announcements) -> Self {
        self.announcements = Some(announcements);
        self
    }

    /// A custom drag layer (react-dnd's `useDragLayer`), for sources whose preview is
    /// [`Preview::Layer`](crate::Preview::Layer). It's called on each frame of their drag, and
    /// what it returns is drawn above everything over the whole window: place it with
    /// `.absolute()` at [`preview_origin`](DragLayerMonitor::preview_origin), which also follows
    /// the preview back to its source after a cancel. It takes no presses.
    pub fn layer(
        mut self,
        layer: impl Fn(&DragLayerMonitor, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.layer = Some(Rc::new(layer));
        self
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
                let window_id = state::window_id(window);
                state.zones.remove(&window_id);
                state
                    .provider_views
                    .insert(window_id, window.current_view());
                state.keyboard.insert(window_id, self.keyboard);
                state.touch.insert(window_id, self.touch);
                match &self.announcements {
                    Some(a) => state.announcements.insert(window_id, a.clone()),
                    None => state.announcements.remove(&window_id),
                };
            }
            outermost
        };
        child.prepaint(window, cx);
        shared.borrow_mut().provider_depth -= 1;
        if outermost {
            draw_preview(self.layer.as_ref(), window, cx);
            draw_live_region(window, cx);
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
        if state::drag_in(window, cx) == Some(false) {
            window.set_window_cursor_style(gpui::CursorStyle::ClosedHand);
        }
        if self.touch {
            crate::touch::listen(window);
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
    // A new press forgets an old one, before the sources under it hear it, and lands a preview
    // on its way back.
    window.on_mouse_event(|_: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture {
            // And ends a keyboard drag: the pointer has taken over.
            match state::drag_in(window, cx) {
                Some(true) => state::land(None, window, cx),
                Some(false)
                    if state::snapshot(cx).is_some_and(|s| s.backend == Backend::Keyboard) =>
                {
                    state::cancel(window, cx);
                    state::land(None, window, cx);
                }
                _ => {}
            }
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

/// Draws the drag's preview, if it has one in this window, above everything else, and asks
/// for the next frame while it's on its way back.
fn draw_preview(layer: Option<&Layer>, window: &mut Window, cx: &mut App) {
    let shared = state::shared(cx);
    let now = cx.background_executor().now();
    let (snap, build, origin, returning) = {
        let state = shared.borrow();
        let Some(drag) = state.drag.as_ref() else {
            return;
        };
        if drag.window != state::window_id(window) {
            return;
        }
        let returning = drag.returning.as_ref().map(|r| r.progress(now) < 1.);
        let build = match &drag.spec.preview {
            PreviewSpec::Default(build) => Some(build.clone()),
            PreviewSpec::Layer => None,
            PreviewSpec::None => return,
        };
        (
            drag.snap.clone(),
            build,
            drag.preview_origin(now),
            returning,
        )
    };
    if returning == Some(true) {
        window.request_animation_frame();
    }
    let (preview, at, space) = match build {
        Some(build) => {
            let preview = match build {
                Some(build) => build(&snap.item, window, cx),
                None => default_preview(&snap.item, snap.source_size, cx),
            };
            let preview = div()
                .debug_selector(|| "dnd-preview".into())
                .child(preview)
                .into_any_element();
            (preview, origin, AvailableSpace::min_size())
        }
        None => {
            let Some(layer) = layer else { return };
            let monitor = DragLayerMonitor {
                monitor: DragMonitor { snap: Some(snap) },
                origin,
            };
            let Some(element) = layer(&monitor, window, cx) else {
                return;
            };
            let viewport = window.viewport_size();
            let layer = div()
                .debug_selector(|| "dnd-layer".into())
                .w(viewport.width)
                .h(viewport.height)
                .child(element)
                .into_any_element();
            let space = Size {
                width: AvailableSpace::Definite(viewport.width),
                height: AvailableSpace::Definite(viewport.height),
            };
            (layer, Point::default(), space)
        }
    };
    let mut floating = Floating {
        child: preview,
        origin: at,
        space,
    }
    .into_any_element();
    floating.request_layout(window, cx);
    window.defer_draw(floating, at, PREVIEW_PRIORITY, None);
}

/// The live region with what the keyboard drag last said, if it said anything in this window.
fn draw_live_region(window: &mut Window, cx: &mut App) {
    let said = state::shared(cx)
        .borrow()
        .said
        .get(&state::window_id(window))
        .cloned();
    let Some(said) = said else { return };
    let mut floating = Floating {
        child: keyboard::live_region(said).into_any_element(),
        origin: Point::default(),
        space: AvailableSpace::min_size(),
    }
    .into_any_element();
    floating.request_layout(window, cx);
    window.defer_draw(floating, Point::default(), PREVIEW_PRIORITY, None);
}

/// An element laid out on its own at a place in the window, as GPUI lays out its own drag's
/// view: deferred draws are only prepainted and painted, so this one's own layout is an empty
/// leaf, and its child is laid out as a root when it's prepainted.
struct Floating {
    child: AnyElement,
    origin: Point<Pixels>,
    space: Size<AvailableSpace>,
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
            .prepaint_as_root(self.origin, self.space, window, cx);
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
