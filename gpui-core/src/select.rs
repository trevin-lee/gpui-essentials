//! A select, as shadcn draws it (its `select.tsx`): a field-like button showing the option
//! chosen (or a placeholder) and a chevron, which opens a [`PopupMenu`] of the options in a
//! [`Popover`], the chosen one checked. It isn't searchable: that's [`Picker`](crate::base::Picker).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Select;
//! # struct Settings { fruit: Option<SharedString> }
//! # impl Render for Settings {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Select::new("fruit")
//!         .placeholder("Pick a fruit")
//!         .heading("Fruit")
//!         .option("apple", "Apple")
//!         .option("banana", "Banana")
//!         .separator()
//!         .heading("Vegetables")
//!         .option("leek", "Leek")
//!         .selected(self.fruit.clone())
//!         .on_change(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.fruit = Some(id.clone());
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement as _, IntoElement, KeyBinding, KeyDownEvent, Modifiers,
    ParentElement as _, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Disableable, Icon, IconName, MenuSelect, Popover, PopupMenu,
    PopupMenuItem, Press, Sizable, focus_ring,
};

actions!(
    gpui_core_select,
    [
        /// Opens the options (Down or Up on the closed select).
        Open
    ]
);

const CONTEXT: &str = "GpuiCoreSelect";

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

enum Entry {
    Option {
        id: SharedString,
        label: SharedString,
        disabled: bool,
    },
    Heading(SharedString),
    Separator,
}

/// A select: options, headings over groups of them and separators between, one option chosen.
/// The host keeps the choice ([`selected`](Select::selected)) and changes it in
/// [`on_change`](Select::on_change); whether it's open is the select's own.
///
/// A press, Enter, Space, Down or Up opens it, on the chosen option; there the menu's keys
/// work (Up and Down, Home and End, Enter or Space to choose, Escape to close), and typing a
/// letter moves to the next option starting with it. On the closed select, a letter chooses
/// the next option starting with it, as a web select does.
#[derive(IntoElement)]
pub struct Select {
    id: ElementId,
    entries: Vec<Entry>,
    selected: Option<SharedString>,
    placeholder: SharedString,
    on_change: Option<OnChange>,
    label: Option<SharedString>,
    disabled: bool,
    invalid: bool,
    size: ControlSize,
    style: StyleRefinement,
}

impl Select {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            entries: Vec::new(),
            selected: None,
            placeholder: SharedString::default(),
            on_change: None,
            label: None,
            disabled: false,
            invalid: false,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    /// An option: `id` is what [`on_change`](Self::on_change) is told; `label` is shown.
    pub fn option(mut self, id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.entries.push(Entry::Option {
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

    /// A group's title, over the options after it: muted, and passed over by the keys.
    pub fn heading(mut self, title: impl Into<SharedString>) -> Self {
        self.entries.push(Entry::Heading(title.into()));
        self
    }

    /// A line between groups.
    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    /// Shows the option `id` dimmed: it can't be chosen, and the keys pass over it.
    pub fn disable(mut self, id: impl Into<SharedString>) -> Self {
        let want = id.into();
        for e in &mut self.entries {
            if let Entry::Option { id, disabled, .. } = e
                && *id == want
            {
                *disabled = true;
            }
        }
        self
    }

    /// The option chosen (`None` for none: the placeholder shows).
    pub fn selected(mut self, id: Option<impl Into<SharedString>>) -> Self {
        self.selected = id.map(Into::into);
        self
    }

    /// What it shows while nothing's chosen, in the muted colour.
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Told the option chosen. The host makes it the selected one.
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// What it chooses, to assistive tech ("Fruit"); it's told the chosen option as its value.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Drawn with the destructive colour's border, as a field that failed its check.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl Disableable for Select {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Select {
    /// Small is 32 points tall, the rest 36; touch makes it and its options at least 44.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Select {
    /// The trigger's style: give it a width (`w_full`, `w(…)`); it's as wide as its text
    /// otherwise.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a select remembers between frames.
#[derive(Default)]
struct SelectState {
    open: bool,
    /// Counts the menus opened: each is a new menu, which opens with its highlight where it's
    /// told and takes the keyboard.
    menu: u64,
    /// The option the menu opens on, and the one highlighted in it since.
    highlighted: Option<SharedString>,
    /// Where typing last moved the highlight (the menu's `move_highlight`).
    typed: Option<SharedString>,
}

/// The option after `from` (going round) whose label starts with `typed`, case aside; from
/// nowhere, the first.
fn next_starting_with(
    options: &[(SharedString, SharedString, bool)],
    from: Option<&SharedString>,
    typed: &str,
) -> Option<SharedString> {
    let typed = typed.to_lowercase();
    let n = options.len();
    let at = from.and_then(|f| options.iter().position(|(id, _, _)| id == f));
    let start = at.map_or(0, |i| i + 1);
    (0..n)
        .map(|k| &options[(start + k) % n])
        .find(|(_, label, enabled)| *enabled && label.to_lowercase().starts_with(&typed))
        .map(|(id, _, _)| id.clone())
}

/// What a key typed: a printable character with no command keys held, other than Space (which
/// chooses).
fn typed(event: &KeyDownEvent) -> Option<String> {
    let m: Modifiers = event.keystroke.modifiers;
    if m.control || m.alt || m.platform || m.function {
        return None;
    }
    // What the key types; for a key reported without it, its own name if that's one
    // character ("b").
    let c = event
        .keystroke
        .key_char
        .clone()
        .unwrap_or_else(|| event.keystroke.key.clone());
    (c.chars().count() == 1 && c.chars().all(|c| !c.is_whitespace() && !c.is_control()))
        .then_some(c)
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let state = window.use_keyed_state(
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "state".into()),
            cx,
            |_, _| SelectState::default(),
        );
        let focus = window
            .use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "focus".into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        let (open, menu_n, highlighted, typed_to) = {
            let s = state.read(cx);
            (
                s.open && !self.disabled,
                s.menu,
                s.highlighted.clone(),
                s.typed.clone(),
            )
        };
        // Every option: its id, label and whether it can be chosen.
        let options: Rc<Vec<(SharedString, SharedString, bool)>> = Rc::new(
            self.entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Option {
                        id,
                        label,
                        disabled,
                    } => Some((id.clone(), label.clone(), !disabled)),
                    _ => None,
                })
                .collect(),
        );
        let chosen = self.selected.as_ref().and_then(|s| {
            options
                .iter()
                .find(|(id, _, _)| id == s)
                .map(|(_, l, _)| l.clone())
        });
        let touch = self.size == ControlSize::Touch;
        let small = matches!(self.size, ControlSize::Small | ControlSize::XSmall);
        let on_change = self.on_change.clone();

        // Opens the menu on the chosen option.
        let opener = {
            let state = state.clone();
            let selected = self.selected.clone();
            move |cx: &mut App| {
                state.update(cx, |s, cx| {
                    s.open = true;
                    s.menu += 1;
                    s.highlighted = selected.clone();
                    s.typed = None;
                    cx.notify();
                })
            }
        };
        let close = {
            let state = state.clone();
            move |cx: &mut App| {
                state.update(cx, |s, cx| {
                    s.open = false;
                    cx.notify();
                })
            }
        };

        let ring = focus_visible(&focus, window) && !touch;
        let border = if self.invalid {
            if dark {
                theme.destructive.opacity(0.5)
            } else {
                theme.destructive
            }
        } else if ring {
            theme.ring
        } else {
            theme.input
        };
        let mut shadows = Vec::new();
        if ring {
            shadows.push(focus_ring(cx));
        } else if self.invalid {
            let mut r = focus_ring(cx);
            r.color = theme.destructive.opacity(if dark { 0.4 } else { 0.2 });
            shadows.push(r);
        }
        let text = chosen.clone().unwrap_or_else(|| self.placeholder.clone());
        let mut trigger = Press::new(ElementId::NamedChild(
            std::sync::Arc::new(self.id.clone()),
            "trigger".into(),
        ))
        .role(Role::ComboBox)
        .aria_expanded(open)
        .when_some(self.label.clone(), |p, l| p.accessibility_label(l))
        .when_some(chosen.clone(), |p, v| p.aria_value(v))
        .track_focus(&focus)
        .focus_ring(false)
        .disabled(self.disabled)
        .debug_selector({
            let id = self.id.clone();
            move || format!("select-{id}")
        })
        .flex()
        .items_center()
        .justify_between()
        .gap(css(6.))
        .min_w_0()
        .max_w_full()
        .h(css(if small { 32. } else { 36. }))
        .when(touch, |p| p.min_h(theme.touch_target))
        .pl(css(10.))
        .pr(css(8.))
        .rounded(theme.radius_md())
        .border_1()
        .border_color(border)
        // The ring (or an invalid field's red one) drawn just outside it, not under it.
        .relative()
        .children({
            let radius = theme.radius_md().to_pixels(window.rem_size());
            shadows
                .into_iter()
                .map(move |s| crate::base::ring_outside(s, radius, gpui::px(1.)))
        })
        .when(dark, |p| p.bg(theme.input.opacity(0.3)))
        .when(dark && !self.disabled, |p| {
            let hover = theme.input.opacity(0.5);
            p.hover(move |s| s.bg(hover))
        })
        .text_size(theme.text.sm)
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
                .when(chosen.is_none(), |d| d.text_color(theme.muted_foreground))
                .child(text),
        )
        .child(
            Icon::new(IconName::ChevronDown)
                .size(css(16.))
                .color(theme.muted_foreground),
        )
        .on_click({
            let (opener, close) = (opener.clone(), close.clone());
            move |_, _, cx| if open { close(cx) } else { opener(cx) }
        });
        gpui::Refineable::refine(trigger.style(), &self.style);

        // The menu: each option, the chosen one checked.
        let selected = self.selected.clone();
        let mut menu = PopupMenu::new(ElementId::NamedInteger("select-menu".into(), menu_n))
            .touch(touch)
            .when_some(highlighted.clone(), |m, h| m.highlight(h))
            .when_some(typed_to, |m, to| m.move_highlight(to))
            .on_highlight({
                let state = state.clone();
                move |id, cx| state.update(cx, |s, _| s.highlighted = id.cloned())
            })
            .on_select({
                let on_change = on_change.clone();
                let close = close.clone();
                move |pick: &MenuSelect, window, cx| {
                    close(cx);
                    if let Some(f) = &on_change {
                        f(&pick.id, window, cx);
                    }
                }
            });
        for e in self.entries {
            menu = match e {
                Entry::Option {
                    id,
                    label,
                    disabled,
                } => {
                    let checked = selected.as_ref() == Some(&id);
                    menu.item(
                        PopupMenuItem::new(id, label)
                            .checked(checked)
                            .disabled(disabled),
                    )
                }
                Entry::Heading(title) => menu.heading(title),
                Entry::Separator => menu.separator(),
            };
        }
        // Typing in the open menu: the highlight to the next option starting with the letter.
        let typeahead = {
            let (state, options) = (state.clone(), options.clone());
            move |event: &KeyDownEvent, _: &mut Window, cx: &mut App| {
                let Some(t) = typed(event) else { return };
                let from = state.read(cx).highlighted.clone();
                if let Some(to) = next_starting_with(&options, from.as_ref(), &t) {
                    cx.stop_propagation();
                    state.update(cx, |s, cx| {
                        if s.typed.as_ref() == Some(&to) {
                            // The menu moves its highlight only to an id other than the last it
                            // was given: back to the same one (the keys moved it away since), it's
                            // a new menu opened there.
                            s.menu += 1;
                            s.typed = None;
                        } else {
                            s.typed = Some(to.clone());
                        }
                        s.highlighted = Some(to);
                        cx.notify();
                    });
                }
            }
        };
        // Typing on the closed select: the next option starting with the letter, chosen.
        let choose_typed = {
            let (options, selected, on_change, state) = (
                options.clone(),
                self.selected.clone(),
                on_change.clone(),
                state.clone(),
            );
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                if state.read(cx).open {
                    return;
                }
                let Some(t) = typed(event) else { return };
                if let Some(to) = next_starting_with(&options, selected.as_ref(), &t) {
                    cx.stop_propagation();
                    if let Some(f) = &on_change {
                        f(&to, window, cx);
                    }
                }
            }
        };

        let disabled = self.disabled;
        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .flex()
            .min_w_0()
            .max_w_full()
            .when(!disabled, |d| {
                d.on_action({
                    let opener = opener.clone();
                    move |_: &Open, _, cx| {
                        if !open {
                            opener(cx)
                        }
                    }
                })
                .on_key_down(choose_typed)
            })
            .child(
                Popover::new(ElementId::NamedChild(
                    std::sync::Arc::new(self.id),
                    "popover".into(),
                ))
                .open(open)
                .role(Role::ListBox)
                .when_some(self.label, |p, l| p.accessibility_label(l))
                .return_focus(&focus)
                .on_dismiss(move |_, _, cx| close(cx))
                .trigger(trigger)
                .p(css(4.))
                .when(open, |p| p.child(div().on_key_down(typeahead).child(menu))),
            )
    }
}

/// Whether `focus` has the focus and the keyboard put it there (the web's `:focus-visible`).
/// GPUI redraws the window when the focus or the kind of input changes, so this is current.
fn focus_visible(focus: &gpui::FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}
