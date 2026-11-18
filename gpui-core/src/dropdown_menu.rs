//! A dropdown menu, as shadcn/ui's: a trigger (a [`Button`], or any [`Press`]) that opens a
//! [`PopupMenu`] hung from it.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, ButtonVariants as _, DropdownMenu, PopoverAlign, PopupMenuItem};
//! # struct Account { picked: Option<SharedString> }
//! # impl Render for Account {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     DropdownMenu::new("account")
//!         .trigger(Button::new("account-button").outline().label("Open"))
//!         .heading("My Account")
//!         .items([
//!             PopupMenuItem::new("profile", "Profile").shortcut("⇧⌘P"),
//!             PopupMenuItem::new("billing", "Billing"),
//!         ])
//!         .separator()
//!         .item(PopupMenuItem::new("log-out", "Log out"))
//!         .align(PopoverAlign::End)
//!         .on_select(cx.listener(|this, pick: &gpui_core::MenuSelect, _, cx| {
//!             // `pick.id` is "profile", "billing" or "log-out"; the menu has closed.
//!             this.picked = Some(pick.id.clone());
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, rems,
};

use crate::base::{
    Disableable, MenuSelect, Popover, PopoverAlign, PopoverSide, PopupMenu, PopupMenuItem, Press,
};
use crate::button::Button;

/// Runs with a pick.
type OnSelect = Rc<dyn Fn(&MenuSelect, &mut Window, &mut App)>;

actions!(
    gpui_core_dropdown_menu,
    [
        /// Opens the menu whose trigger has the keyboard, its first entry highlighted.
        Open
    ]
);

const CONTEXT: &str = "GpuiCoreDropdownMenu";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("down", Open, Some(CONTEXT))]);
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// What opens a [`DropdownMenu`]: a [`Button`] or a [`Press`]. Its own `on_click`, if it had
/// one, is replaced by the menu's.
pub struct DropdownMenuTrigger(TriggerKind);

// Both boxed: the two differ in size by a different amount on each target (pointers are 4
// bytes on wasm32), so boxing only the larger one would trip clippy somewhere.
enum TriggerKind {
    Button(Box<Button>),
    Press(Box<Press>),
}

impl From<Button> for DropdownMenuTrigger {
    fn from(button: Button) -> Self {
        Self(TriggerKind::Button(Box::new(button)))
    }
}

impl From<Press> for DropdownMenuTrigger {
    fn from(press: Press) -> Self {
        Self(TriggerKind::Press(Box::new(press)))
    }
}

/// A trigger and the [`PopupMenu`] it opens, below it and lined up with its start unless told
/// otherwise ([`side`](Self::side), [`align`](Self::align)).
///
/// A click, a tap, Enter or Space on the trigger opens and closes it, and Down opens it. From the
/// keyboard it opens with its first entry highlighted; from the pointer with none, until the
/// keys or the pointer move. A tap opens it with rows 44 points tall. A pick runs
/// [`on_select`](Self::on_select) and closes it; Escape and a press outside close it too, and
/// the keyboard goes back to the trigger. Assistive tech hears the trigger as expanded while
/// it's open.
///
/// It keeps whether it's open under its id: give each its own.
#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    trigger: Option<DropdownMenuTrigger>,
    menu: PopupMenu,
    on_select: Option<OnSelect>,
    side: PopoverSide,
    align: PopoverAlign,
    touch: bool,
    disabled: bool,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl DropdownMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            menu: PopupMenu::new(ElementId::NamedChild(
                std::sync::Arc::new(id.clone()),
                "menu".into(),
            )),
            id,
            trigger: None,
            on_select: None,
            side: PopoverSide::Bottom,
            align: PopoverAlign::Start,
            touch: false,
            disabled: false,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// What opens it: a [`Button`], or a [`Press`] drawn as the host likes.
    pub fn trigger(mut self, trigger: impl Into<DropdownMenuTrigger>) -> Self {
        self.trigger = Some(trigger.into());
        self
    }

    pub fn item(mut self, item: PopupMenuItem) -> Self {
        self.menu = self.menu.item(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = PopupMenuItem>) -> Self {
        self.menu = self.menu.items(items);
        self
    }

    pub fn separator(mut self) -> Self {
        self.menu = self.menu.separator();
        self
    }

    /// A section's title between entries (shadcn's `DropdownMenuLabel`).
    pub fn heading(mut self, title: impl Into<SharedString>) -> Self {
        self.menu = self.menu.heading(title);
        self
    }

    /// Runs with the pick (the entry's id and the modifiers held), after the menu has closed.
    /// A submenu's picks come here too, unless it has an `on_select` of its own.
    pub fn on_select(mut self, f: impl Fn(&MenuSelect, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Which side of the trigger the menu opens on (below unless given). It goes to the other
    /// side where there's no room.
    pub fn side(mut self, side: PopoverSide) -> Self {
        self.side = side;
        self
    }

    /// Where the menu lines up along that side: with the trigger's start (unless given), its
    /// centre or its end.
    pub fn align(mut self, align: PopoverAlign) -> Self {
        self.align = align;
        self
    }

    /// Rows 44 points tall however it's opened (a tap opens it with them anyway).
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }

    /// What the open menu is called to assistive tech, when the trigger's label doesn't say.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Disableable for DropdownMenu {
    /// The trigger is drawn disabled and nothing opens.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The box around the trigger: its place in a row, say.
impl Styled for DropdownMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// How an open menu was opened.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Opened {
    /// From the keyboard: its first entry highlighted.
    keys: bool,
    /// By a tap: rows for touch.
    touch: bool,
}

struct State {
    open: Option<Opened>,
}

fn set(state: &Entity<State>, open: Option<Opened>, cx: &mut App) {
    state.update(cx, |s, cx| {
        if s.open != open {
            s.open = open;
            cx.notify();
        }
    });
}

impl RenderOnce for DropdownMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| State { open: None });
        let open = if self.disabled {
            None
        } else {
            state.read(cx).open
        };
        let is_open = open.is_some();
        let child = |name: &'static str| {
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), name.into())
        };

        let toggle = {
            let state = state.clone();
            move |e: &ClickEvent, window: &mut Window, cx: &mut App| {
                let how = Opened {
                    keys: matches!(e, ClickEvent::Keyboard(_)),
                    // GPUI hands a tap over as a mouse click.
                    touch: matches!(e, ClickEvent::Touch(_)) || window.last_input_was_touch(),
                };
                let now = state.read(cx).open;
                set(&state, if now.is_some() { None } else { Some(how) }, cx);
            }
        };
        let disabled = self.disabled;
        let trigger = self.trigger.map(|t| match t.0 {
            TriggerKind::Button(b) => {
                let b = (*b).expanded(is_open).on_click(toggle);
                if disabled { b.disabled(true) } else { b }.into_any_element()
            }
            TriggerKind::Press(p) => press_trigger(*p, is_open, disabled)
                .on_click(toggle)
                .into_any_element(),
        });

        let menu = open.map(|how| {
            let menu = self.menu.start_blank(!how.keys);
            let host = self.on_select.clone();
            let state = state.clone();
            menu.touch(self.touch || how.touch)
                .on_select(move |pick, window, cx| {
                    set(&state, None, cx);
                    if let Some(f) = &host {
                        f(pick, window, cx);
                    }
                })
        });

        let dismiss = state.clone();
        let mut popover = Popover::new(child("popover"))
            .open(menu.is_some())
            .role(Role::Group)
            .match_trigger_width(false)
            .side(self.side)
            .align(self.align)
            .on_dismiss(move |_, _, cx| set(&dismiss, None, cx))
            .min_w(css(128.))
            .p(css(4.))
            .children(menu);
        if let Some(trigger) = trigger {
            popover = popover.trigger(trigger);
        }
        if let Some(label) = self.label {
            popover = popover.accessibility_label(label);
        }

        let keys = state.clone();
        let mut wrapper = div()
            .id(child("dropdown"))
            .key_context(CONTEXT)
            .on_action(move |_: &Open, _, cx| {
                // Open, Down is the menu's.
                if disabled || keys.read(cx).open.is_some() {
                    cx.propagate();
                    return;
                }
                set(
                    &keys,
                    Some(Opened {
                        keys: true,
                        touch: false,
                    }),
                    cx,
                );
            })
            .flex()
            .min_w_0()
            .child(popover);
        gpui::Refineable::refine(wrapper.style(), &self.style);
        wrapper
    }
}

/// What the menu adds to a [`Press`] trigger: whether it's expanded, and disabled.
fn press_trigger(p: Press, open: bool, disabled: bool) -> Press {
    let p = p.aria_expanded(open);
    if disabled { p.disabled(true) } else { p }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    #[gpui::test]
    fn a_press_trigger_says_whether_it_is_expanded(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let focus = cx.focus_handle();
            for open in [false, true] {
                let el = press_trigger(
                    Press::new("p").track_focus(&focus).child("Open"),
                    open,
                    false,
                )
                .render(window, cx)
                .into_element();
                let mut node = accesskit::Node::new(Role::Unknown);
                el.write_a11y_info(&mut node);
                assert_eq!(el.a11y_role(), Some(Role::Button));
                assert_eq!(node.is_expanded(), Some(open));
            }
        });
    }
}
