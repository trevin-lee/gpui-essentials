//! The `dnd` element: a div that's a drag source, a drop target, or both.

use std::rc::Rc;
use std::sync::Arc;

use gpui::Refineable as _;
use gpui::{
    AnyElement, App, BorderStyle, Bounds, DispatchPhase, Div, Element, ElementId, FocusHandle,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement, Interactivity,
    IntoElement, LayoutId, LongPressEvent, MouseButton, MouseDownEvent, ParentElement, Pixels,
    RenderOnce, SharedString, Stateful, StatefulInteractiveElement, StyleRefinement, Styled,
    TouchDragEvent, TouchPhase, Window, div, quad, rems, transparent_black,
};
use gpui_core::ActiveTheme as _;

use crate::monitor::{DragMonitor, Over};
use crate::source::{DragSource, SourceSpec};
use crate::state::{self, Pending, SourceRecord, TargetRecord};
use crate::target::DropTarget;

/// What VoiceOver says a source the keyboard can pick up does.
pub const PICK_UP_HINT: &str = "Press Space to pick up.";

/// A div that's a drag source, a drop target, or both (a sortable item is both), like
/// react-dnd's `drag(drop(ref))`. Its id is its source's and target's id, which the
/// [`DragMonitor`] answers about; it must be unique in its window.
///
/// ```ignore
/// dnd(("card", card.id))
///     .source(DragSource::item(card.clone()).on_end(|end, _, cx| { /* … */ }))
///     .target(DropTarget::new().accept::<Card>().on_drop(|item, monitor, _, cx| {
///         DropOutcome::Handled(None)
///     }))
///     .dragging_style(|s| s.opacity(0.4))
///     .child(card.title.clone())
/// ```
pub fn dnd(id: impl Into<ElementId>) -> Dnd {
    let id = id.into();
    Dnd {
        base: div().id(id.clone()),
        id,
        source: None,
        target: None,
        dragging_style: None,
        over_style: None,
        can_drop_style: None,
        indicator: true,
        label: None,
    }
}

/// See [`dnd`].
#[derive(IntoElement)]
pub struct Dnd {
    base: Stateful<Div>,
    id: ElementId,
    source: Option<Box<dyn FnOnce(Option<SharedString>) -> SourceSpec>>,
    target: Option<DropTarget>,
    dragging_style: Option<Box<StyleRefinement>>,
    over_style: Option<Box<StyleRefinement>>,
    can_drop_style: Option<Box<StyleRefinement>>,
    indicator: bool,
    label: Option<SharedString>,
}

impl Dnd {
    /// Makes it a drag source.
    pub fn source<T: 'static>(mut self, source: DragSource<T>) -> Self {
        self.source = Some(Box::new(move |label| source.erase(label)));
        self
    }

    /// Makes it a drop target.
    pub fn target(mut self, target: DropTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Styles while its own item is the one dragged (react-dnd's collected `isDragging`):
    /// typically `|s| s.opacity(0.4)`.
    pub fn dragging_style(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.dragging_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Styles while an item it can take is over it, shallowly.
    pub fn over_style(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.over_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Styles while an item it can take is being dragged, wherever the pointer is.
    pub fn can_drop_style(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.can_drop_style = Some(Box::new(f(StyleRefinement::default())));
        self
    }

    /// Whether a target draws the theme's ring round itself while an item it can take is over
    /// it, shallowly. On unless turned off.
    pub fn drop_indicator(mut self, on: bool) -> Self {
        self.indicator = on;
        self
    }

    /// Its accessible name: the element's, and the item's in announcements.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for Dnd {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for Dnd {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements)
    }
}

impl InteractiveElement for Dnd {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Dnd {}

impl RenderOnce for Dnd {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let monitor = DragMonitor::of(cx);
        let source = self.source.map(|make| Rc::new(make(self.label.clone())));
        let mut base = self.base;
        if let Some(label) = &self.label {
            base = base.aria_label(label.clone());
        }
        // A source the keyboard can pick up is a Tab stop with the theme's focus ring.
        let focus = source.as_ref().filter(|s| s.keyboard).map(|_| {
            window
                .use_keyed_state(
                    ElementId::NamedChild(Arc::new(self.id.clone()), "dnd-focus".into()),
                    cx,
                    |_, cx| cx.focus_handle(),
                )
                .read(cx)
                .clone()
        });
        if let Some(focus) = &focus {
            let ring = gpui_core::focus_ring(cx);
            base = base
                .track_focus(&focus.clone().tab_stop(true))
                .focus_visible(move |s| s.shadow(vec![ring]))
                // A press doesn't take the focus (GPUI's focus-on-press checks this): a focus
                // change draws every view anew, and a drag by mouse needn't move the keyboard.
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .aria_description(PICK_UP_HINT);
        }
        let mut over = false;
        if let Some(item) = monitor.item() {
            if let Some(spec) = &source {
                let dragging = match &spec.is_dragging {
                    Some(f) => f(item),
                    None => monitor.is_dragging_source(&self.id),
                };
                if let (true, Some(style)) = (dragging, &self.dragging_style) {
                    base.style().refine(style);
                }
            }
            if let Some(target) = &self.target
                && target.takes(item, &monitor, cx)
            {
                if let Some(style) = &self.can_drop_style {
                    base.style().refine(style);
                }
                over = monitor.is_over(&self.id, Over::Shallow);
                if let (true, Some(style)) = (over, &self.over_style) {
                    base.style().refine(style);
                }
            }
        }
        Registrar {
            id: self.id,
            child: Some(base.into_any_element()),
            source,
            focus,
            target: self.target.map(Rc::new),
            label: self.label,
            indicator: over && self.indicator,
        }
    }
}

/// Registers its child's bounds as a source and a target while laying it out, and listens for
/// the press that starts a drag. It adds no box of its own.
struct Registrar {
    id: ElementId,
    child: Option<AnyElement>,
    source: Option<Rc<SourceSpec>>,
    focus: Option<FocusHandle>,
    target: Option<Rc<DropTarget>>,
    label: Option<SharedString>,
    indicator: bool,
}

/// What a `dnd` element keeps between frames: its records, so they live while it's drawn.
struct Held {
    _source: Option<Rc<SourceRecord>>,
    _target: Option<Rc<TargetRecord>>,
}

impl IntoElement for Registrar {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Registrar {
    type RequestLayoutState = AnyElement;
    type PrepaintState = (Option<Hitbox>, Option<Rc<SourceRecord>>);

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let window_id = state::window_id(window);
        let shared = state::shared(cx);
        let visible = bounds.intersect(&window.content_mask().bounds);
        let (source, target) = {
            let mut state = shared.borrow_mut();
            let source = self.source.clone().map(|spec| {
                Rc::new(SourceRecord {
                    id: self.id.clone(),
                    bounds,
                    spec,
                    focus: self.focus.clone(),
                })
            });
            let target = self.target.clone().map(|spec| {
                let parent = state.target_stack.last().map(Rc::downgrade);
                Rc::new(TargetRecord {
                    id: self.id.clone(),
                    bounds,
                    visible,
                    seq: state.next_seq(),
                    parent,
                    spec,
                    label: self.label.clone(),
                })
            });
            let registry = state.registry(window_id);
            if let Some(s) = &source {
                registry.add_source(s);
            }
            if let Some(t) = &target {
                registry.add_target(t);
                state.target_stack.push(t.clone());
            }
            (source, target)
        };
        if let Some(global_id) = global_id {
            let held = Held {
                _source: source.clone(),
                _target: target.clone(),
            };
            window.with_element_state(global_id, |_: Option<Held>, _| ((), held));
        }
        let hitbox = source
            .is_some()
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal));
        child.prepaint(window, cx);
        if target.is_some() {
            shared.borrow_mut().target_stack.pop();
        }
        (hitbox, source)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        (hitbox, source): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
        if self.indicator {
            let theme = cx.theme();
            let radius = theme.radius_md().to_pixels(window.rem_size());
            window.paint_quad(quad(
                bounds,
                radius,
                transparent_black(),
                gpui::px(2.),
                theme.ring,
                BorderStyle::Solid,
            ));
        }
        if let (Some(hitbox), Some(source)) = (hitbox.clone(), source.clone()) {
            // Touch: the innermost source under the finger hears it first, and claims it.
            let (h, s) = (hitbox.clone(), source.clone());
            window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.phase == TouchPhase::Started
                    && h.is_hovered_at(event.start_position, window)
                    && crate::touch::long_press(&s, event, window, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
            let (h, s) = (hitbox.clone(), source.clone());
            window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.phase == TouchPhase::Started
                    && h.is_hovered_at(event.start_position, window)
                    && crate::touch::offer(&s, event, window, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !hitbox.is_hovered(window)
                    || cx.has_active_drag()
                {
                    return;
                }
                let shared = state::shared(cx);
                let mut state = shared.borrow_mut();
                // The innermost source under the press hears it first and takes it.
                if state.drag.is_some() || state.pending.is_some() {
                    return;
                }
                let threshold = source
                    .spec
                    .threshold
                    .unwrap_or(rems(0.25))
                    .to_pixels(window.rem_size());
                state.pending = Some(Pending {
                    source: source.clone(),
                    at: event.position,
                    threshold,
                    window: state::window_id(window),
                });
            });
        }
    }
}
