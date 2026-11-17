//! A combobox, as shadcn/ui's: a button showing the option chosen (or a placeholder) that opens
//! a [`Popover`] with a search field over the options, filtered as you type. It's a
//! [`Picker`] under a trigger; a short list with no search is a [`Select`](crate::Select).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Combobox;
//! # struct Project { framework: Option<SharedString> }
//! # impl Render for Project {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Combobox::new("framework")
//!         .placeholder("Select framework...")
//!         .search_placeholder("Search framework...")
//!         .empty_text("No framework found.")
//!         .options([("next", "Next.js"), ("svelte", "SvelteKit"), ("astro", "Astro")])
//!         .selected(self.framework.clone())
//!         .on_change(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.framework = Some(id.clone());
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FontWeight, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::menu::{MenuSelect, PopupMenuItem};
use crate::picker::Picker;
use crate::popover::Popover;
use crate::press::Press;
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::ActiveTheme as _;

actions!(
    gpui_core_combobox,
    [
        /// Opens the list (Down or Up on the closed combobox).
        Open
    ]
);

const CONTEXT: &str = "GpuiCoreCombobox";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("down", Open, c),
        KeyBinding::new("up", Open, c),
    ]);
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

type OnChange = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

struct Choice {
    id: SharedString,
    label: SharedString,
    disabled: bool,
}

/// A searchable select. The host keeps the choice ([`selected`](Combobox::selected)) and
/// changes it in [`on_change`](Combobox::on_change); whether it's open is the combobox's own.
///
/// A press, a tap, Enter, Space, Down or Up opens it, the keyboard in its search field and the
/// chosen option highlighted. Typing filters the options (every word, case aside), highlighting
/// the first match, or shows the [`empty_text`](Self::empty_text) when none does; Up and Down
/// move through the matches and Enter or a press chooses. Choosing closes it and gives the
/// keyboard back to the trigger, as Escape and a press outside do. The trigger is 200 pixels
/// wide unless styled otherwise, and the list as wide as it: long labels are cut short with an
/// ellipsis in both.
#[derive(IntoElement)]
pub struct Combobox {
    id: ElementId,
    choices: Vec<Choice>,
    selected: Option<SharedString>,
    placeholder: SharedString,
    search_placeholder: SharedString,
    empty: SharedString,
    on_change: Option<OnChange>,
    label: Option<SharedString>,
    disabled: bool,
    size: ControlSize,
    style: StyleRefinement,
}

impl Combobox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            choices: Vec::new(),
            selected: None,
            placeholder: "Select...".into(),
            search_placeholder: "Search...".into(),
            empty: "No results found.".into(),
            on_change: None,
            label: None,
            disabled: false,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    /// An option: `id` is what [`on_change`](Self::on_change) is told; `label` is shown and
    /// searched.
    pub fn option(mut self, id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.choices.push(Choice {
            id: id.into(),
            label: label.into(),
            disabled: false,
        });
        self
    }

    /// Many options at once, as `(id, label)`.
    pub fn options<I: Into<SharedString>, L: Into<SharedString>>(
        mut self,
        options: impl IntoIterator<Item = (I, L)>,
    ) -> Self {
        for (id, label) in options {
            self = self.option(id, label);
        }
        self
    }

    /// Shows the option `id` dimmed: it can't be chosen, and the keys pass over it.
    pub fn disable(mut self, id: impl Into<SharedString>) -> Self {
        let want = id.into();
        for c in &mut self.choices {
            if c.id == want {
                c.disabled = true;
            }
        }
        self
    }

    /// The option chosen (`None` for none: the placeholder shows).
    pub fn selected(mut self, id: Option<impl Into<SharedString>>) -> Self {
        self.selected = id.map(Into::into);
        self
    }

    /// What the trigger shows while nothing's chosen ("Select..." unless given).
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The search field's placeholder ("Search..." unless given).
    pub fn search_placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.search_placeholder = text.into();
        self
    }

    /// What the list shows when nothing matches ("No results found." unless given).
    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    /// Told the option chosen. The host makes it the selected one.
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// What it chooses, to assistive tech ("Framework"); it's told the chosen option as its
    /// value.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Disableable for Combobox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Combobox {
    /// Small is 32 points tall, the rest 36; touch makes it and its options at least 44.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Combobox {
    /// The trigger's style: its width (200 pixels unless given), which the list takes too.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a combobox remembers between frames.
#[derive(Default)]
struct ComboboxState {
    open: bool,
    /// Counts the lists opened: each is a new picker, with an empty search, the keyboard in it.
    picker: u64,
}

impl RenderOnce for Combobox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let child = |name: &'static str| {
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), name.into())
        };
        let state = window.use_keyed_state(child("state"), cx, |_, _| ComboboxState::default());
        let focus = window
            .use_keyed_state(child("focus"), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let (open, picker_n) = {
            let s = state.read(cx);
            (s.open && !self.disabled, s.picker)
        };
        let chosen = self.selected.as_ref().and_then(|s| {
            self.choices
                .iter()
                .find(|c| &c.id == s)
                .map(|c| c.label.clone())
        });
        let touch = self.size == ControlSize::Touch;
        let small = matches!(self.size, ControlSize::Small | ControlSize::XSmall);

        let opener = {
            let state = state.clone();
            move |cx: &mut App| {
                state.update(cx, |s, cx| {
                    s.open = true;
                    s.picker += 1;
                    cx.notify();
                })
            }
        };
        let close = {
            let state = state.clone();
            move |cx: &mut App| {
                state.update(cx, |s, cx| {
                    if s.open {
                        s.open = false;
                        cx.notify();
                    }
                })
            }
        };

        // shadcn's trigger: an outline button, pressed in while open.
        let (fill, border, hover) = match (dark, open) {
            (true, false) => (
                theme.input.opacity(0.3),
                theme.input,
                theme.input.opacity(0.5),
            ),
            (true, true) => (theme.input, theme.input, theme.input),
            (false, false) => (theme.background, theme.border, theme.muted),
            (false, true) => (theme.muted, theme.border, theme.muted),
        };
        let text = chosen.clone().unwrap_or_else(|| self.placeholder.clone());
        let mut trigger = Press::new(child("trigger"))
            .role(Role::ComboBox)
            .aria_expanded(open)
            .when_some(self.label.clone(), |p, l| p.accessibility_label(l))
            .when_some(chosen.clone(), |p, v| p.aria_value(v))
            .track_focus(&focus)
            .disabled(self.disabled)
            .debug_selector({
                let id = self.id.clone();
                move || format!("combobox-{id}")
            })
            .flex()
            .items_center()
            .justify_between()
            .gap(css(6.))
            .w(css(200.))
            .min_w_0()
            .max_w_full()
            .h(css(if small { 32. } else { 36. }))
            .when(touch, |p| p.min_h(theme.touch_target))
            .px(css(10.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(border)
            .bg(fill)
            .shadow_xs()
            .text_color(theme.foreground)
            .text_size(theme.text.sm)
            .font_weight(FontWeight::MEDIUM)
            .when(!self.disabled, |p| p.hover(move |s| s.bg(hover)))
            .when(self.disabled, |p| p.opacity(0.5))
            .child(
                div()
                    .min_w_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    // Room either side for ink past a glyph's advance.
                    .px(css(3.))
                    .mx(css(-3.))
                    .debug_selector({
                        let id = self.id.clone();
                        move || format!("combobox-{id}-text")
                    })
                    .child(text),
            )
            .child(
                div().flex_none().child(
                    Icon::new(IconName::ChevronsUpDown)
                        .size(css(16.))
                        .color(theme.muted_foreground),
                ),
            )
            .on_click({
                let (opener, close) = (opener.clone(), close.clone());
                move |_, _, cx| if open { close(cx) } else { opener(cx) }
            });
        gpui::Refineable::refine(trigger.style(), &self.style);

        let picker = open.then(|| {
            let on_change = self.on_change.clone();
            let close = close.clone();
            let selected = self.selected.clone();
            Picker::new(ElementId::NamedInteger("combobox-picker".into(), picker_n))
                .placeholder(self.search_placeholder.clone())
                .empty_text(self.empty.clone())
                .touch(touch)
                .items(self.choices.into_iter().map(|c| {
                    let checked = selected.as_ref() == Some(&c.id);
                    PopupMenuItem::new(c.id, c.label)
                        .checked(checked)
                        .disabled(c.disabled)
                }))
                .on_select(move |pick: &MenuSelect, window, cx| {
                    close(cx);
                    if let Some(f) = &on_change {
                        f(&pick.id, window, cx);
                    }
                })
        });

        let disabled = self.disabled;
        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .flex()
            .min_w_0()
            .max_w_full()
            .when(!disabled, |d| {
                d.on_action(move |_: &Open, _, cx| {
                    if open {
                        cx.propagate();
                    } else {
                        opener(cx)
                    }
                })
            })
            .child(
                Popover::new(child("popover"))
                    .open(open)
                    .when_some(self.label, |p, l| p.accessibility_label(l))
                    .return_focus(&focus)
                    .fit_trigger_width(true)
                    .on_dismiss(move |_, _, cx| close(cx))
                    .trigger(trigger)
                    .p(css(4.))
                    .children(picker),
            )
    }
}
