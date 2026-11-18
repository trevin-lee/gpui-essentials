//! A context menu: a [`PopupMenu`] that opens where an element is pressed with the secondary
//! button (a right-click, a two-finger click), where a finger rests on it, or from the keyboard
//! (Shift-F10, or the Menu key where there is one) under the element that has the focus.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{ContextMenu, IconName, MenuSelect, PopupMenu, PopupMenuItem, Press};
//! # struct Files { picked: Option<SharedString> }
//! # impl Render for Files {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     let this = cx.entity().downgrade();
//!     ContextMenu::new("notes-row", Press::new("notes").child("Notes.md"))
//!         .menu(move |_, _| {
//!             let this = this.clone();
//!             PopupMenu::new("notes-menu")
//!                 .item(PopupMenuItem::new("rename", "Rename"))
//!                 .item(PopupMenuItem::new("delete", "Delete").icon(IconName::Trash))
//!                 .on_select(move |pick: &MenuSelect, _, cx| {
//!                     // `pick.id` is "rename" or "delete"; the menu closes by itself.
//!                     let id = pick.id.clone();
//!                     this.update(cx, |files, cx| {
//!                         files.picked = Some(id);
//!                         cx.notify();
//!                     })
//!                     .ok();
//!                 })
//!         })
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, Entity, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, InteractiveElement as _, IntoElement, KeyBinding, LayoutId,
    LongPressEvent, MouseButton, MouseDownEvent, ParentElement as _, Pixels, Point, RenderOnce,
    Role, StyleRefinement, Styled, TouchPhase, Window, actions, div, px, rems,
};

use crate::base::{Popover, PopupMenu};

actions!(
    gpui_core_context_menu,
    [
        /// Opens the context menu of the element with the focus, under it.
        Show
    ]
);

const CONTEXT: &str = "GpuiCoreContextMenu";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("shift-f10", Show, c),
        // The Menu (application) key, on keyboards that have one.
        KeyBinding::new("menu", Show, c),
    ]);
}

type BuildMenu = Rc<dyn Fn(&mut Window, &mut App) -> PopupMenu>;
type OnOpenChange = Rc<dyn Fn(bool, &mut App)>;

/// Wraps an element (a list's row, a file, a doc) and opens a [`PopupMenu`] over everything
/// else:
///
/// - where it's pressed with the secondary button (a right-click, a two-finger click);
/// - where a finger rests on it without moving (GPUI's long press: half a second, within a few
///   points), with rows 44 points tall;
/// - from the keyboard, Shift-F10 or the Menu key, while it or anything in it has the focus:
///   under its bottom-left corner.
///
/// It closes on a pick (after the menu's [`on_select`](PopupMenu::on_select) runs, which its
/// submenus' picks go to as well), on Escape, on a press outside it, and when the element
/// scrolls out of view; the focus then goes back to what had it. A plain click, a tap, and
/// selecting text in the element are the element's own.
///
/// The menu is made by [`menu`](Self::menu) only while it's open. The element keeps the focus
/// for the keys: make it something that takes the focus (a [`Press`](crate::base::Press)) to reach
/// the menu from the keyboard. Give each element its own id: what's open is kept under it.
#[derive(IntoElement)]
pub struct ContextMenu {
    id: ElementId,
    child: AnyElement,
    menu: Option<BuildMenu>,
    on_open_change: Option<OnOpenChange>,
    style: StyleRefinement,
}

impl ContextMenu {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            menu: None,
            on_open_change: None,
            style: StyleRefinement::default(),
        }
    }

    /// Told `true` when the menu opens and `false` when it closes, however it does (a pick,
    /// Escape, a press outside, its element scrolled away). For a host with something drawn
    /// over the window that the menu must not go under, such as a native web view it hides
    /// while a menu is up.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Makes the menu, each time it's drawn while open. Without one nothing opens.
    pub fn menu(mut self, build: impl Fn(&mut Window, &mut App) -> PopupMenu + 'static) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }
}

/// The box around the element: its size and place in a flex row, say. The element is drawn
/// inside it.
impl Styled for ContextMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Where an open menu hangs from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Opened {
    /// The pointer or a finger, from the element's top-left corner; a finger's menu has rows
    /// for touch.
    At { offset: Point<Pixels>, touch: bool },
    /// The keys: under the element's bottom-left corner.
    Keys,
}

/// What a context menu remembers between frames: whether it's open, and where, and who's told
/// when that changes.
struct State {
    open: Option<Opened>,
    on_open_change: Option<OnOpenChange>,
}

fn set(state: &Entity<State>, open: Option<Opened>, cx: &mut App) {
    let told = state.update(cx, |s, cx| {
        if s.open == open {
            return None;
        }
        let was = s.open.is_some();
        s.open = open;
        cx.notify();
        (was != open.is_some())
            .then(|| s.on_open_change.clone())
            .flatten()
    });
    if let Some(f) = told {
        f(open.is_some(), cx);
    }
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| State {
            open: None,
            on_open_change: None,
        });
        state.update(cx, |s, _| s.on_open_change = self.on_open_change.clone());
        let open = state.read(cx).open;
        let menu = match (open, &self.menu) {
            (Some(opened), Some(build)) => {
                let mut menu = build(window, cx);
                if matches!(opened, Opened::At { touch: true, .. }) {
                    menu = menu.touch(true);
                }
                // A pick closes it, after the host's handler.
                let state = state.clone();
                Some(menu.after_select(move |_, _, cx| set(&state, None, cx)))
            }
            _ => None,
        };

        // What the popover hangs from: a point where it was pressed, or a strip along the
        // element's bottom edge (a strip, not a point on the edge, so it's in view while the
        // element's last row of pixels is).
        let marker = match open {
            Some(Opened::At { offset, .. }) => div().absolute().left(offset.x).top(offset.y),
            Some(Opened::Keys) | None => div().absolute().left_0().bottom_0().w_full().h(px(1.)),
        };
        let dismiss = state.clone();
        let popover = Popover::new("context-menu")
            .open(menu.is_some())
            .trigger(marker)
            .match_trigger_width(false)
            .role(Role::Group)
            .on_dismiss(move |_, _, cx| set(&dismiss, None, cx))
            .p(rems(0.25))
            .children(menu);

        let shown = state.clone();
        let has_menu = self.menu.is_some();
        let mut wrapper = div()
            .id(self.id)
            .relative()
            .key_context(CONTEXT)
            .on_action(move |_: &Show, _, cx| {
                // Open, Shift-F10 in the menu does nothing.
                if has_menu && shown.read(cx).open.is_none() {
                    set(&shown, Some(Opened::Keys), cx);
                }
            })
            .child(self.child)
            .child(popover);
        gpui::Refineable::refine(wrapper.style(), &self.style);

        Pressed {
            child: wrapper.into_any_element(),
            state: self.menu.is_some().then_some(state),
        }
    }
}

/// Draws the element and opens its menu on a secondary press or a long press on it.
struct Pressed {
    child: AnyElement,
    /// None without a menu: nothing opens.
    state: Option<Entity<State>>,
}

impl IntoElement for Pressed {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Pressed {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

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
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Before the element's own listeners, so a context menu inside it (a cell's, in a row
        // with one) hears a press first and keeps it.
        if let Some(state) = &self.state {
            let origin = bounds.origin;
            window.on_mouse_event({
                let (state, hitbox) = (state.clone(), hitbox.clone());
                move |e: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble
                        || e.button != MouseButton::Right
                        || !hitbox.is_hovered(window)
                    {
                        return;
                    }
                    let offset = e.position - origin;
                    set(
                        &state,
                        Some(Opened::At {
                            offset,
                            touch: false,
                        }),
                        cx,
                    );
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event({
                let (state, hitbox) = (state.clone(), hitbox.clone());
                move |e: &LongPressEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble
                        || e.phase != TouchPhase::Started
                        || window.default_prevented()
                        || !hitbox.is_hovered(window)
                    {
                        return;
                    }
                    let offset = e.start_position - origin;
                    set(
                        &state,
                        Some(Opened::At {
                            offset,
                            touch: true,
                        }),
                        cx,
                    );
                    // Claimed: lifting the finger is the long press's end, not a tap.
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
        }
        self.child.paint(window, cx);
    }
}
