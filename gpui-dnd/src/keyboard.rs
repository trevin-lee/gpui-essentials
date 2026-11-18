//! The keyboard backend, which react-dnd lacks: Space or Enter picks up the focused source, the
//! arrows move to the nearest target in that direction that can take the item, Space or Enter
//! drops it there and Escape cancels. A keyboard drag holds the keyboard: no other key reaches
//! the app while it's on. Each step is said in the provider's live region.

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement as _, KeystrokeEvent, Modifiers, Pixels, Point, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use crate::item::DragItem;
use crate::monitor::DragMonitor;
use crate::source::Backend;
use crate::state::{self, TargetRecord};

/// A sentence about the item at a target (its label, if it has one).
pub type SayAt = Rc<dyn Fn(&DragItem, Option<&SharedString>) -> SharedString>;

/// What the live region says at each step of a keyboard drag. Each sentence is made from the
/// item (its label, from [`Dnd::label`](crate::Dnd::label)) and the target's label.
#[derive(Clone)]
pub struct Announcements {
    pub picked_up: Rc<dyn Fn(&DragItem) -> SharedString>,
    /// Over a target (its label), or over none that can take it.
    pub over: SayAt,
    pub dropped: SayAt,
    pub cancelled: Rc<dyn Fn(&DragItem) -> SharedString>,
}

/// The item as a sentence names it: its label, "3 items", or "the item".
pub fn item_name(item: &DragItem) -> SharedString {
    match item.label() {
        Some(label) if item.count() < 2 => label.clone(),
        _ if item.count() > 1 => format!("{} items", item.count()).into(),
        _ => "the item".into(),
    }
}

impl Default for Announcements {
    fn default() -> Self {
        Self {
            picked_up: Rc::new(|item| {
                format!(
                    "Picked up {}. Arrow keys move it, Space drops it, Escape cancels.",
                    item_name(item)
                )
                .into()
            }),
            over: Rc::new(|_, target| match target {
                Some(target) => format!("Over {target}.").into(),
                None => "Over a place it can go.".into(),
            }),
            dropped: Rc::new(|item, target| match target {
                Some(target) => format!("Dropped {} on {target}.", item_name(item)).into(),
                None => format!("Dropped {}.", item_name(item)).into(),
            }),
            cancelled: Rc::new(|item| {
                format!("Cancelled. {} is back where it was.", item_name(item)).into()
            }),
        }
    }
}

/// A key, before the app sees it. True if the drag took it.
pub(crate) fn key(event: &KeystrokeEvent, window: &mut Window, cx: &mut App) -> bool {
    let stroke = &event.keystroke;
    let plain = !stroke.modifiers.modified();
    let key = stroke.key.as_str();
    match state::drag_in(window, cx) {
        // On its way back: Escape lands it, other keys go on.
        Some(true) => {
            if key == "escape" {
                state::land(None, window, cx);
                return true;
            }
            false
        }
        Some(false) => {
            if key == "escape" {
                state::cancel(window, cx);
                return true;
            }
            let Some(snap) = state::snapshot(cx) else {
                return false;
            };
            if snap.backend != Backend::Keyboard {
                return false;
            }
            match key {
                "space" | "enter" if plain => {
                    state::release(snap.current, window, cx);
                }
                "up" if plain => step(point(0., -1.), window, cx),
                "down" if plain => step(point(0., 1.), window, cx),
                "left" if plain => step(point(-1., 0.), window, cx),
                "right" if plain => step(point(1., 0.), window, cx),
                _ => {}
            }
            true
        }
        None => {
            if !plain || !(key == "space" || key == "enter") {
                return false;
            }
            pick_up(window, cx)
        }
    }
}

fn point(x: f32, y: f32) -> Point<f32> {
    Point { x, y }
}

/// Picks up the source that has the focus, at its centre.
fn pick_up(window: &mut Window, cx: &mut App) -> bool {
    let shared = state::shared(cx);
    let window_id = state::window_id(window);
    let source = {
        let mut state = shared.borrow_mut();
        if !state.keyboard.get(&window_id).copied().unwrap_or(false) {
            return false;
        }
        state.registry(window_id).focused_source(window)
    };
    let Some(source) = source else {
        return false;
    };
    if !state::begin(
        &source,
        source.bounds.center(),
        Backend::Keyboard,
        window,
        cx,
    ) {
        return false;
    }
    if let Some(snap) = state::snapshot(cx) {
        let said = announcements(window, cx).picked_up.clone();
        announce(said(&snap.item), window, cx);
        if let Some(drag) = shared.borrow_mut().drag.as_mut() {
            drag.said_over = Some(None);
        }
    }
    true
}

/// Moves to the nearest target in `direction` that can take the item: the one whose anchor is
/// least far along plus twice as far across.
fn step(direction: Point<f32>, window: &mut Window, cx: &mut App) {
    let shared = state::shared(cx);
    let window_id = state::window_id(window);
    let Some(snap) = state::snapshot(cx) else {
        return;
    };
    let targets = shared.borrow_mut().registry(window_id).live_targets();
    let monitor = DragMonitor::of(cx);
    let from = snap.current;
    let mut best: Option<(f32, Point<Pixels>)> = None;
    for target in targets {
        let visible = target.visible;
        if visible.size.width <= px(0.)
            || visible.size.height <= px(0.)
            || !target.spec.takes(&snap.item, &monitor, cx)
        {
            continue;
        }
        let at = target.spec.anchor(visible);
        let d = at - from;
        let (dx, dy) = (f32::from(d.x), f32::from(d.y));
        let along = dx * direction.x + dy * direction.y;
        let across = (dx * direction.y - dy * direction.x).abs();
        if along < 1. {
            continue;
        }
        let score = along + 2. * across;
        if best.is_none_or(|(b, _)| score < b) {
            best = Some((score, at));
        }
    }
    let Some((_, at)) = best else {
        return;
    };
    state::move_to(at, Modifiers::none(), window, cx);
    announce_over(window, cx);
}

/// Says which target the drag is over, if that changed.
fn announce_over(window: &mut Window, cx: &mut App) {
    let shared = state::shared(cx);
    let over = {
        let mut state = shared.borrow_mut();
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        let over: Option<&Rc<TargetRecord>> = drag.targets.last();
        let id: Option<ElementId> = over.map(|t| t.id.clone());
        if drag.said_over.as_ref() == Some(&id) {
            return;
        }
        drag.said_over = Some(id);
        (drag.snap.item.clone(), over.and_then(|t| t.label.clone()))
    };
    let said = announcements(window, cx).over.clone();
    announce(said(&over.0, over.1.as_ref()), window, cx);
}

pub(crate) fn announcements(window: &Window, cx: &mut App) -> Announcements {
    state::shared(cx)
        .borrow()
        .announcements
        .get(&state::window_id(window))
        .cloned()
        .unwrap_or_default()
}

/// What this window's live region last said, for hosts that route announcements themselves
/// and for tests.
pub fn announcement(window: &Window, cx: &App) -> Option<SharedString> {
    state::said(window, cx)
}

/// Puts `text` in this window's live region.
pub(crate) fn announce(text: SharedString, window: &mut Window, cx: &mut App) {
    state::shared(cx)
        .borrow_mut()
        .said
        .insert(state::window_id(window), text);
    state::redraw(window, cx);
}

/// The live region: a status node that assistive technology reads when its label changes.
pub(crate) fn live_region(text: SharedString) -> Stateful<gpui::Div> {
    div()
        .id("dnd-announcer")
        .debug_selector(|| "dnd-announcer".into())
        .role(Role::Status)
        .aria_label(text)
        .size(px(1.))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::accesskit;
    use gpui::{Element as _, IntoElement as _};

    #[test]
    fn the_live_region_is_a_status_with_the_text() {
        let el = live_region("Over Done.".into()).into_element();
        assert_eq!(el.a11y_role(), Some(Role::Status));
        let mut node = accesskit::Node::new(Role::Status);
        el.write_a11y_info(&mut node);
        assert_eq!(node.label(), Some("Over Done."));
    }

    #[test]
    fn the_item_is_named_by_its_label_or_count() {
        assert_eq!(
            item_name(&DragItem::new(vec![1], Some("Card".into()))),
            "Card"
        );
        assert_eq!(item_name(&DragItem::new(vec![1, 2, 3], None)), "3 items");
        assert_eq!(item_name(&DragItem::new(vec![1], None)), "the item");
    }
}
