//! A picker: a filtering PopupMenu.

use std::rc::Rc;

use gpui::{
    App, AppContext as _, ElementId, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::input::{Input, InputState};
use crate::menu::{Entry, OnSelect, PopupMenuColors, PopupMenuItem, rows, step};
use crate::theme::ActiveTheme as _;

actions!(gpui_core_picker, [Previous, Next, Choose]);

/// The picker's field: its keys over the field's own (a one-line field's Up and Down would
/// otherwise go to its start and end, and Enter would submit it).
const FIELD: &str = "GpuiCorePicker > GpuiCoreInput";

pub(crate) fn init(cx: &mut App) {
    let c = Some(FIELD);
    cx.bind_keys([
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("down", Next, c),
        KeyBinding::new("enter", Choose, c),
    ]);
}

/// A picker: a [`PopupMenu`](crate::PopupMenu) with a field at the top that filters its
/// entries as you type. Typing goes to the field; Up and Down move the highlight through what
/// matches and Enter picks it, while the field keeps the keyboard. In a
/// [`Popover`](crate::Popover), Escape closes it.
///
/// The host owns what's picked and whether its popover is open: a single-choice picker closes
/// it in [`Picker::on_select`], a several-choice one keeps it open and checks what's picked.
///
/// ```no_run
/// # use gpui::*;
/// # use gpui_core::{Picker, PopupMenuItem, Popover, Button};
/// # struct Panel { open: bool, tag: SharedString }
/// # impl Render for Panel {
/// fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
///     Popover::new("tag")
///         .open(self.open)
///         .on_dismiss(cx.listener(|this, _, _, cx| {
///             this.open = false;
///             cx.notify();
///         }))
///         .trigger(Button::new("tag-button").label(self.tag.clone()).on_click(cx.listener(
///             |this, _, _, cx| {
///                 this.open = !this.open;
///                 cx.notify();
///             },
///         )))
///         .child(
///             Picker::new("tag-picker")
///                 .placeholder("Find a tag")
///                 .items(["Work", "Home", "Errands"].map(|t| {
///                     PopupMenuItem::new(t, t).checked(self.tag.as_ref() == t)
///                 }))
///                 .on_select(cx.listener(|this, id: &SharedString, _, cx| {
///                     this.tag = id.clone();
///                     this.open = false;
///                     cx.notify();
///                 })),
///         )
/// }
/// # }
/// ```
#[derive(IntoElement)]
pub struct Picker {
    id: ElementId,
    items: Vec<PopupMenuItem>,
    placeholder: SharedString,
    empty: SharedString,
    on_select: Option<OnSelect>,
    touch: bool,
    colors: PopupMenuColors,
}

impl Picker {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            placeholder: "Search".into(),
            empty: "No matches".into(),
            on_select: None,
            touch: false,
            colors: PopupMenuColors::default(),
        }
    }

    pub fn item(mut self, item: PopupMenuItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = PopupMenuItem>) -> Self {
        self.items.extend(items);
        self
    }

    /// The field's placeholder ("Search" unless given).
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// What it shows when nothing matches ("No matches" unless given).
    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    /// Runs with the picked entry's id.
    pub fn on_select(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Entries 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }

    pub fn colors(mut self, colors: PopupMenuColors) -> Self {
        self.colors = colors;
        self
    }
}

/// What a picker remembers between frames.
struct PickerState {
    query: Entity<InputState>,
    /// The query the highlight was set for.
    seen: Option<SharedString>,
    highlighted: Option<usize>,
}

/// Whether `label` matches `query`: every word of the query is in it, case aside.
fn matches(label: &str, query: &str) -> bool {
    let label = label.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|w| label.contains(w))
}

impl RenderOnce for Picker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let placeholder = self.placeholder.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            // The field takes the keyboard when the picker first shows.
            let q = query.clone();
            window.defer(cx, move |window, cx| {
                q.update(cx, |q, cx| q.focus(window, cx))
            });
            PickerState {
                query,
                seen: None,
                highlighted: None,
            }
        });
        let query = state.read(cx).query.clone();
        let text = query.read(cx).value();

        let shown: Vec<PopupMenuItem> = self
            .items
            .into_iter()
            .filter(|i| matches(&i.label, &text))
            .collect();
        let pickable: Rc<Vec<bool>> = Rc::new(shown.iter().map(|i| !i.disabled).collect());
        // Where it opens: on the current option (the checked one), else on the first match.
        let current = shown
            .iter()
            .position(|i| i.checked == Some(true) && !i.disabled);
        let ids: Rc<Vec<SharedString>> = Rc::new(shown.iter().map(|i| i.id.clone()).collect());

        // Opening highlights the current option, a new query the first match; the pointer and
        // the keys move it from there.
        let highlighted = state.update(cx, |s, _| {
            if s.seen.is_none() {
                s.seen = Some(text.clone());
                s.highlighted = current.or_else(|| step(&pickable, None, 1));
            } else if s.seen.as_ref() != Some(&text) {
                s.seen = Some(text.clone());
                s.highlighted = step(&pickable, None, 1);
            }
            s.highlighted.filter(|i| *i < pickable.len())
        });

        let mover = |delta: isize| {
            let state = state.clone();
            let pickable = pickable.clone();
            move |_: &mut Window, cx: &mut App| {
                state.update(cx, |s, cx| {
                    let to = step(&pickable, s.highlighted, delta);
                    if to != s.highlighted {
                        s.highlighted = to;
                        cx.notify();
                    }
                })
            }
        };
        let (prev, next) = (mover(-1), mover(1));
        let choose = {
            let state = state.clone();
            let on_select = self.on_select.clone();
            let pickable = pickable.clone();
            move |window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let (Some(i), Some(f)) = (at, &on_select)
                    && pickable.get(i) == Some(&true)
                    && let Some(id) = ids.get(i)
                {
                    f(id, window, cx);
                }
            }
        };

        let theme = cx.theme().clone();
        let muted = self.colors.muted.unwrap_or(theme.muted_foreground);
        let empty = shown.is_empty();
        let list = rows(
            &self.id,
            shown.into_iter().map(Entry::Item).collect(),
            highlighted,
            self.touch,
            self.colors,
            Rc::new({
                let state = state.clone();
                move |i, cx: &mut App| {
                    state.update(cx, |s, cx| {
                        if s.highlighted != Some(i) {
                            s.highlighted = Some(i);
                            cx.notify();
                        }
                    })
                }
            }),
            self.on_select.clone(),
            cx,
        );

        div()
            .id(self.id)
            .key_context("GpuiCorePicker")
            .flex()
            .flex_col()
            .min_w_0()
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &Choose, w, cx| choose(w, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(6. / 15.))
                    .px(rems(8. / 15.))
                    .mb(rems(4. / 15.))
                    .border_b_1()
                    .border_color(self.colors.separator.unwrap_or(theme.border))
                    .child(Icon::new(IconName::Search).color(muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&query).appearance(false)),
                    ),
            )
            .child(
                div()
                    .id("picker-list")
                    .role(Role::ListBox)
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .when(empty, |d| {
                        d.child(
                            div()
                                .px(rems(8. / 15.))
                                .py(rems(6. / 15.))
                                .text_size(theme.text.sm)
                                .text_color(muted)
                                .child(self.empty),
                        )
                    })
                    .children(list),
            )
    }
}
