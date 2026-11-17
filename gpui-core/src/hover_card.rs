//! A hover card: a card of more about something (a person, a link), shown when the pointer
//! rests on it and when it has the keyboard's focus.
//!
//! Nothing works by hover alone, so the card shows while its trigger has the focus too, and
//! Escape hides it. It's placed like a [`Popover`](crate::Popover) and isn't modal: the focus
//! stays on the trigger. Since the card isn't reachable from the keyboard, put nothing in it
//! that can't be done elsewhere.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, ButtonVariants as _, HoverCard};
//! # struct Panel;
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     HoverCard::new("author")
//!         .trigger(Button::new("author-link").link().label("@nextjs"))
//!         .child("The React Framework, created and maintained by @vercel.")
//! }
//! # }
//! ```

use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, rems,
};

use crate::popover::{Dismiss, Popover, PopoverColors};
use crate::theme::ActiveTheme as _;

/// How long the pointer rests on the trigger before the card shows (the web's 700 ms).
pub const HOVER_CARD_OPEN_DELAY: Duration = Duration::from_millis(700);
/// How long the card stays after the pointer leaves it and its trigger (the web's 300 ms), so
/// the pointer can cross from one to the other.
pub const HOVER_CARD_CLOSE_DELAY: Duration = Duration::from_millis(300);

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A trigger drawn in place and a card hung from it, shown after the pointer has rested on the
/// trigger for [`open_delay`](HoverCard::open_delay), and while the trigger (or something in it)
/// has the focus. It hides [`close_delay`](HoverCard::close_delay) after the pointer has left
/// both the trigger and the card, when the focus leaves, on Escape, and on a press outside.
///
/// It keeps whether it's shown itself, under its id: nothing redraws while nothing changes, and
/// a timer runs only between the pointer coming or going and the card showing or hiding.
#[derive(IntoElement)]
pub struct HoverCard {
    id: ElementId,
    trigger: Option<AnyElement>,
    open_delay: Duration,
    close_delay: Duration,
    colors: Option<PopoverColors>,
    label: Option<SharedString>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl HoverCard {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger: None,
            open_delay: HOVER_CARD_OPEN_DELAY,
            close_delay: HOVER_CARD_CLOSE_DELAY,
            colors: None,
            label: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// What it's about, drawn in place: something that takes the focus (a link, a button), so
    /// the keyboard can show the card too.
    pub fn trigger(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// How long the pointer rests on the trigger before the card shows
    /// ([`HOVER_CARD_OPEN_DELAY`] by default).
    pub fn open_delay(mut self, delay: Duration) -> Self {
        self.open_delay = delay;
        self
    }

    /// How long the card stays once the pointer has left it and its trigger
    /// ([`HOVER_CARD_CLOSE_DELAY`] by default).
    pub fn close_delay(mut self, delay: Duration) -> Self {
        self.close_delay = delay;
        self
    }

    /// The card's colours: by default the theme's `popover`, `popover-foreground` and `border`.
    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for HoverCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The card's own style: its width (16 rem by default), padding, gap.
impl Styled for HoverCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a hover card remembers between frames.
struct HoverState {
    /// Around the trigger: what it holds having the focus shows the card.
    focus: FocusHandle,
    on_trigger: bool,
    on_card: bool,
    /// Shown for the pointer (its open delay passed, its close delay not).
    by_pointer: bool,
    /// Hidden by Escape or a press outside while the focus stays: shown again only once the
    /// focus has left and come back, or the pointer comes back.
    hidden: bool,
    /// Bumped on every pointer change, so only the latest timer acts.
    generation: u64,
}

/// The pointer came onto or left the trigger (`card` false) or the card, at `generation`:
/// after the delay, if nothing has changed since, the card shows or hides for it.
fn pointer(
    state: &Entity<HoverState>,
    card: bool,
    on: bool,
    delays: (Duration, Duration),
    window: &mut Window,
    cx: &mut App,
) {
    let (generation, wanted, changes) = state.update(cx, |s, _| {
        if card {
            s.on_card = on;
        } else {
            s.on_trigger = on;
        }
        s.generation += 1;
        let wanted = s.on_trigger || s.on_card;
        (s.generation, wanted, wanted != s.by_pointer)
    });
    if !changes {
        return;
    }
    let delay = if wanted { delays.0 } else { delays.1 };
    let state = state.clone();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(delay).await;
            cx.update(|_, cx| {
                state.update(cx, |s, cx| {
                    if s.generation == generation {
                        s.by_pointer = wanted;
                        if wanted {
                            s.hidden = false;
                        }
                        cx.notify();
                    }
                })
            })
            .ok();
        })
        .detach();
}

impl RenderOnce for HoverCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let key = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "hover".into());
        let state = window.use_keyed_state(key, cx, |_, cx| HoverState {
            focus: cx.focus_handle(),
            on_trigger: false,
            on_card: false,
            by_pointer: false,
            hidden: false,
            generation: 0,
        });
        let focus = state.read(cx).focus.clone();
        let focused = focus.contains_focused(window, cx) && !focus.is_focused(window);
        // The focus left: Escape's hiding is over.
        if !focused && state.read(cx).hidden && !state.read(cx).by_pointer {
            state.update(cx, |s, _| s.hidden = false);
        }
        let open = {
            let s = state.read(cx);
            (s.by_pointer || focused) && !s.hidden
        };
        let delays = (self.open_delay, self.close_delay);
        let theme = cx.theme();

        let on_trigger = state.clone();
        let on_escape = state.clone();
        let trigger = div()
            .id("hover-card-trigger")
            .track_focus(&focus)
            .key_context(crate::popover::CONTEXT)
            .on_hover(move |hovered, window, cx| {
                pointer(&on_trigger, false, *hovered, delays, window, cx)
            })
            .on_action(move |_: &Dismiss, _, cx| {
                if !open {
                    // Not ours: the host's Escape.
                    cx.propagate();
                    return;
                }
                on_escape.update(cx, |s, cx| {
                    s.by_pointer = false;
                    s.hidden = true;
                    s.generation += 1;
                    cx.notify();
                });
            })
            .children(self.trigger);

        let on_card = state.clone();
        let mut card = div()
            .id("hover-card")
            .w(css(256.))
            .p(css(16.))
            .text_size(theme.text.sm)
            .debug_selector(|| "gpui-core-hover-card".into())
            .on_hover(move |hovered, window, cx| {
                pointer(&on_card, true, *hovered, delays, window, cx)
            });
        gpui::Refineable::refine(card.style(), &self.style);
        let card = card.children(self.children);

        let on_dismiss = state.clone();
        let mut popover = Popover::new(self.id)
            .open(open)
            .take_focus(false)
            .match_trigger_width(false)
            .role(Role::Tooltip)
            .on_dismiss(move |_, _, cx| {
                on_dismiss.update(cx, |s, cx| {
                    s.by_pointer = false;
                    s.on_card = false;
                    s.hidden = true;
                    s.generation += 1;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .child(card);
        if let Some(colors) = self.colors {
            popover = popover.colors(colors);
        }
        if let Some(label) = self.label {
            popover = popover.accessibility_label(label);
        }
        popover
    }
}
