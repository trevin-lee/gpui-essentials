//! A picker: a filtering PopupMenu.

use std::rc::Rc;

use gpui::{
    App, AppContext as _, ElementId, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    Modifiers, ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::input::{Input, InputState};
use crate::menu::{
    Entry, MenuSelect, MoveHighlight, OnHighlight, OnSelect, PopupMenuColors, PopupMenuItem,
    RowStyle, rows, secondary, shift, step,
};
use crate::theme::ActiveTheme as _;

/// Past this many matches a picker draws only the rows in view (relations, people).
pub(crate) const VIRTUAL_AFTER: usize = 100;

/// The Create entry's id: not one a host would use.
const CREATE: &str = "\u{0}gpui-core-picker-create";

type OnCreate = Rc<dyn Fn(&str, &mut Window, &mut App)>;

actions!(
    gpui_core_picker,
    [Previous, Next, Choose, ChooseShift, ChooseSecondary]
);

/// The picker's field: its keys over the field's own (a one-line field's Up and Down would
/// otherwise go to its start and end, and Enter would submit it).
const FIELD: &str = "GpuiCorePicker > GpuiCoreInput";

pub(crate) fn init(cx: &mut App) {
    let c = Some(FIELD);
    cx.bind_keys([
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("down", Next, c),
        KeyBinding::new("enter", Choose, c),
        KeyBinding::new("shift-enter", ChooseShift, c),
        KeyBinding::new(
            if cfg!(any(target_os = "macos", target_os = "ios")) {
                "cmd-enter"
            } else {
                "ctrl-enter"
            },
            ChooseSecondary,
            c,
        ),
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
/// # use gpui_core::{Button, MenuSelect, Picker, PopupMenuItem, Popover};
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
///                 .on_select(cx.listener(|this, pick: &MenuSelect, _, cx| {
///                     this.tag = pick.id.clone();
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
    on_highlight: Option<OnHighlight>,
    highlight: Option<SharedString>,
    row: RowStyle,
    on_create: Option<OnCreate>,
    visible_rows: Option<usize>,
    colors: PopupMenuColors,
    /// The search field's caret, selection and placeholder colours.
    field: (Option<gpui::Hsla>, Option<gpui::Hsla>, Option<gpui::Hsla>),
}

impl Picker {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            placeholder: "Search".into(),
            empty: "No matches".into(),
            on_select: None,
            on_highlight: None,
            highlight: None,
            row: RowStyle::default(),
            on_create: None,
            visible_rows: None,
            colors: PopupMenuColors::default(),
            field: (None, None, None),
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
    pub fn on_select(mut self, f: impl Fn(&MenuSelect, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The entry highlighted when it opens (the checked one, else the first match, unless
    /// given).
    pub fn highlight(mut self, id: impl Into<SharedString>) -> Self {
        self.highlight = Some(id.into());
        self
    }

    /// Told the highlighted entry's id when the keys, the pointer or a new query move the
    /// highlight (not when it opens).
    pub fn on_highlight(mut self, f: impl Fn(Option<&SharedString>, &mut App) + 'static) -> Self {
        self.on_highlight = Some(Rc::new(f));
        self
    }

    /// Entries 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.row.touch = touch;
        self
    }

    /// A row's height (32 CSS pixels unless given; at least 44 points with
    /// [`touch`](Self::touch)).
    pub fn row_height(mut self, height: impl Into<gpui::AbsoluteLength>) -> Self {
        self.row.height = Some(height.into());
        self
    }

    /// The entries' text size (the theme's small size unless given).
    pub fn text_size(mut self, size: impl Into<gpui::AbsoluteLength>) -> Self {
        self.row.text_size = Some(size.into());
        self
    }

    /// Puts the check in a column before each label, as [`PopupMenu::check_leading`](crate::PopupMenu::check_leading).
    pub fn check_leading(mut self, leading: bool) -> Self {
        self.row.check_leading = leading;
        self
    }

    /// The search field's caret colour (the theme's foreground unless given).
    pub fn caret_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.field.0 = Some(color.into());
        self
    }

    /// The search field's selection colour (the theme's ring at 30% unless given).
    pub fn selection_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.field.1 = Some(color.into());
        self
    }

    /// The search field's placeholder colour (the theme's muted foreground unless given).
    pub fn placeholder_color(mut self, color: impl Into<gpui::Hsla>) -> Self {
        self.field.2 = Some(color.into());
        self
    }

    /// Offers a "Create “text”" entry after the matches while what's typed isn't an entry's
    /// label already (case aside); picking it runs `f` with the text, trimmed.
    /// How many rows a long list shows before it scrolls (10, or 8 for touch, unless given).
    /// A list of more than a hundred matches draws only the rows in view.
    pub fn visible_rows(mut self, rows: usize) -> Self {
        self.visible_rows = Some(rows.max(1));
        self
    }

    pub fn on_create(mut self, f: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_create = Some(Rc::new(f));
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
    /// A long list's scroll, so the keys keep the highlight in view.
    scroll: gpui::UniformListScrollHandle,
    /// Whether it has scrolled to where it opens.
    opened: bool,
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
                scroll: gpui::UniformListScrollHandle::new(),
                opened: false,
                seen: None,
                highlighted: None,
            }
        });
        let query = state.read(cx).query.clone();
        let text = query.read(cx).value();

        let typed = text.trim().to_string();
        let exact = self
            .items
            .iter()
            .any(|i| i.label.to_lowercase() == typed.to_lowercase());
        let mut shown: Vec<PopupMenuItem> = self
            .items
            .into_iter()
            .filter(|i| matches(&i.label, &text))
            .collect();
        if self.on_create.is_some() && !typed.is_empty() && !exact {
            shown
                .push(PopupMenuItem::new(CREATE, format!("Create “{typed}”")).icon(IconName::Plus));
        }
        // A pick, or the Create entry's.
        let on_select: Option<OnSelect> = match (self.on_select.clone(), self.on_create.clone()) {
            (None, None) => None,
            (select, create) => Some(Rc::new(move |pick: &MenuSelect, window, cx| {
                if pick.id.as_ref() == CREATE {
                    if let Some(f) = &create {
                        f(&typed, window, cx);
                    }
                } else if let Some(f) = &select {
                    f(pick, window, cx);
                }
            })),
        };
        let pickable: Rc<Vec<bool>> = Rc::new(shown.iter().map(|i| !i.disabled).collect());
        // Where it opens: on the host's entry, else the current option (the checked one), else
        // the first match.
        let given = self
            .highlight
            .as_ref()
            .and_then(|want| shown.iter().position(|i| &i.id == want && !i.disabled));
        let current = given.or_else(|| {
            shown
                .iter()
                .position(|i| i.checked == Some(true) && !i.disabled)
        });
        let ids: Rc<Vec<SharedString>> = Rc::new(shown.iter().map(|i| i.id.clone()).collect());

        // Opening highlights the current option, a new query the first match; the pointer and
        // the keys move it from there.
        let (highlighted, requeried) = state.update(cx, |s, _| {
            let mut requeried = false;
            if s.seen.is_none() {
                s.seen = Some(text.clone());
                s.highlighted = current.or_else(|| step(&pickable, None, 1));
            } else if s.seen.as_ref() != Some(&text) {
                s.seen = Some(text.clone());
                s.highlighted = step(&pickable, None, 1);
                requeried = true;
            }
            if (requeried || !s.opened)
                && let Some(i) = s.highlighted
            {
                s.scroll.scroll_to_item(i, gpui::ScrollStrategy::Nearest);
            }
            s.opened = true;
            (s.highlighted.filter(|i| *i < pickable.len()), requeried)
        });
        // A new query moved it: tell the host after this frame (not in the middle of a draw).
        if requeried && let Some(f) = self.on_highlight.clone() {
            let id = highlighted.map(|i| ids[i].clone());
            cx.defer(move |cx| f(id.as_ref(), cx));
        }
        // Moves the highlight to `to`, and tells the host if it moved.
        let set: MoveHighlight = Rc::new({
            let state = state.clone();
            let ids = ids.clone();
            let on_highlight = self.on_highlight.clone();
            move |to, cx: &mut App| {
                let moved = state.update(cx, |s, cx| {
                    let moved = to != s.highlighted;
                    if moved {
                        s.highlighted = to;
                        if let Some(i) = to {
                            s.scroll.scroll_to_item(i, gpui::ScrollStrategy::Nearest);
                        }
                        cx.notify();
                    }
                    moved
                });
                if moved && let Some(f) = &on_highlight {
                    f(to.and_then(|i| ids.get(i)), cx);
                }
            }
        });

        let mover = |delta: isize| {
            let state = state.clone();
            let pickable = pickable.clone();
            let set = set.clone();
            move |_: &mut Window, cx: &mut App| {
                let from = state.read(cx).highlighted;
                set(step(&pickable, from, delta), cx)
            }
        };
        let (prev, next) = (mover(-1), mover(1));
        let choose = Rc::new({
            let state = state.clone();
            let on_select = on_select.clone();
            let pickable = pickable.clone();
            let ids = ids.clone();
            move |modifiers: Modifiers, window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let (Some(i), Some(f)) = (at, &on_select)
                    && pickable.get(i) == Some(&true)
                    && let Some(id) = ids.get(i)
                {
                    let pick = MenuSelect {
                        id: id.clone(),
                        modifiers,
                    };
                    f(&pick, window, cx);
                }
            }
        });
        let (choose_shift, choose_secondary) = (choose.clone(), choose.clone());

        let theme = cx.theme().clone();
        let muted = self.colors.muted.unwrap_or(theme.muted_foreground);
        let empty = shown.is_empty();
        let count = shown.len();
        let set_highlight: crate::menu::SetHighlight =
            Rc::new(move |i, cx: &mut App| set(Some(i), cx));
        let list: gpui::AnyElement = if count > VIRTUAL_AFTER {
            // Only the rows in view are made, from entries taken as they're shown.
            let rem = window.rem_size();
            let row_px = self
                .row
                .height
                .map(|h| h.to_pixels(rem))
                .unwrap_or(rem * (32. / 15.));
            let row_px = if self.row.touch {
                row_px.max(theme.touch_target)
            } else {
                row_px
            };
            let visible = self
                .visible_rows
                .unwrap_or(if self.row.touch { 8 } else { 10 });
            let entries: Rc<std::cell::RefCell<Vec<Option<Entry>>>> = Rc::new(
                std::cell::RefCell::new(shown.into_iter().map(|i| Some(Entry::Item(i))).collect()),
            );
            let (id, row, colors) = (self.id.clone(), self.row, self.colors);
            let scroll = state.read(cx).scroll.clone();
            gpui::uniform_list("picker-rows", count, move |range, _, cx| {
                let taken: Vec<Entry> = {
                    let mut e = entries.borrow_mut();
                    if range == (0..1) {
                        // The list measures its rows on the first, before drawing the ones in
                        // view: a text-only copy, leaving the entry to be drawn.
                        e.first()
                            .and_then(|e| match e {
                                Some(Entry::Item(i)) => Some(Entry::Item(i.text_copy())),
                                _ => None,
                            })
                            .into_iter()
                            .collect()
                    } else {
                        range
                            .clone()
                            .filter_map(|i| e.get_mut(i).and_then(Option::take))
                            .collect()
                    }
                };
                rows(
                    &id,
                    taken,
                    range.start,
                    highlighted,
                    row,
                    colors,
                    set_highlight.clone(),
                    on_select.clone(),
                    cx,
                )
            })
            .track_scroll(&scroll)
            .h(row_px * visible.min(count) as f32)
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .children(rows(
                    &self.id,
                    shown.into_iter().map(Entry::Item).collect(),
                    0,
                    highlighted,
                    self.row,
                    self.colors,
                    set_highlight,
                    on_select,
                    cx,
                ))
                .into_any_element()
        };

        div()
            .id(self.id)
            .key_context("GpuiCorePicker")
            .flex()
            .flex_col()
            .min_w_0()
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &Choose, w, cx| choose(Modifiers::none(), w, cx))
            .on_action(move |_: &ChooseShift, w, cx| choose_shift(shift(), w, cx))
            .on_action(move |_: &ChooseSecondary, w, cx| choose_secondary(secondary(cx), w, cx))
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
                    .child(div().flex_1().min_w_0().child({
                        let (caret, selection, placeholder) = self.field;
                        Input::new(&query)
                            .appearance(false)
                            .when_some(caret, |i, c| i.caret_color(c))
                            .when_some(selection, |i, c| i.selection_color(c))
                            .when_some(placeholder, |i, c| i.placeholder_color(c))
                    })),
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
                    .child(list),
            )
    }
}
