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

/// How many times a `uniform_list` (in the GPUI this is pinned to) measures its first row before
/// it asks for the rows in view: in its layout and again as it's placed.
const MEASURES_PER_DRAW: usize = 2;

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
    /// Each group's heading, with the index of its first entry.
    headings: Vec<(usize, SharedString)>,
    placeholder: SharedString,
    empty: SharedString,
    on_select: Option<OnSelect>,
    on_highlight: Option<OnHighlight>,
    highlight: Option<SharedString>,
    row: RowStyle,
    on_create: Option<OnCreate>,
    visible_rows: Option<usize>,
    filter: bool,
    colors: PopupMenuColors,
    /// The search field's caret, selection and placeholder colours.
    field: (Option<gpui::Hsla>, Option<gpui::Hsla>, Option<gpui::Hsla>),
    /// The text the field starts with.
    query: Option<SharedString>,
    max_chars: Option<usize>,
    /// Whether the field shows a magnifying glass (while it filters, unless given).
    search_icon: Option<bool>,
}

impl Picker {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            headings: Vec::new(),
            placeholder: "Search".into(),
            empty: "No matches".into(),
            on_select: None,
            on_highlight: None,
            highlight: None,
            row: RowStyle::default(),
            on_create: None,
            visible_rows: None,
            filter: true,
            colors: PopupMenuColors::default(),
            field: (None, None, None),
            query: None,
            max_chars: None,
            search_icon: None,
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

    /// Starts a group: a heading over the entries added after it (up to the next heading),
    /// shown while any of them match. A list of more than a hundred matches is drawn without
    /// its headings.
    pub fn heading(mut self, title: impl Into<SharedString>) -> Self {
        self.headings.push((self.items.len(), title.into()));
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

    /// Whether typing filters the entries (on by default). Off, the field names something while
    /// the list stays whole and its highlight where the keys put it: a new property's name over
    /// its types, Enter picking the type with the name in [`MenuSelect::query`].
    pub fn filter(mut self, filter: bool) -> Self {
        self.filter = filter;
        self
    }

    /// How many rows a long list shows before it scrolls (10, or 8 for touch, unless given).
    /// A list of more than a hundred matches draws only the rows in view.
    pub fn visible_rows(mut self, rows: usize) -> Self {
        self.visible_rows = Some(rows.max(1));
        self
    }

    /// The text the field starts with, its caret at the end: what was typed before, when a
    /// host comes back to this step. Taken when the picker first shows; after that the text is
    /// the person's.
    pub fn query(mut self, text: impl Into<SharedString>) -> Self {
        self.query = Some(text.into());
        self
    }

    /// The most characters the field takes, as [`InputState::max_chars`].
    pub fn max_chars(mut self, max: usize) -> Self {
        self.max_chars = Some(max);
        self
    }

    /// Whether the field shows a magnifying glass: by default while it filters (not on a
    /// naming field, [`filter(false)`](Self::filter)).
    pub fn search_icon(mut self, show: bool) -> Self {
        self.search_icon = Some(show);
        self
    }

    /// Offers a "Create “text”" entry after the matches while what's typed isn't an entry's
    /// label already (case aside); picking it runs `f` with the text, trimmed.
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
    /// The highlighted entry's index among those shown this frame, worked out from its id.
    highlighted: Option<usize>,
    /// The highlighted entry, by its id: it stays that entry's while the host's entries come, go
    /// and move, so Enter picks the entry that's shown highlighted.
    highlighted_id: Option<SharedString>,
}

/// The entry, if it's one to pick (not a heading).
fn as_item(e: &Entry) -> Option<&PopupMenuItem> {
    match e {
        Entry::Item(i) => Some(i),
        _ => None,
    }
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
        let start = self.query.clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            let query = cx.new(|cx| {
                let q = InputState::new(window, cx).placeholder(placeholder);
                match start {
                    Some(text) => q.default_value(text.to_string()),
                    None => q,
                }
            });
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
                highlighted_id: None,
            }
        });
        let query = state.read(cx).query.clone();
        // The host's limit, as it is this frame (setting it draws nothing).
        let max_chars = self.max_chars;
        query.update(cx, |q, _| q.set_max_chars(max_chars));
        let text = query.read(cx).value();

        let typed = text.trim().to_string();
        let exact = self
            .items
            .iter()
            .any(|i| i.label.to_lowercase() == typed.to_lowercase());
        let filter = self.filter;
        let mut items: Vec<PopupMenuItem> = Vec::new();
        // Each heading kept, with the index among the matches of its group's first.
        let mut heads: Vec<(usize, SharedString)> = Vec::new();
        let mut headings = self.headings.into_iter().peekable();
        for (i, item) in self.items.into_iter().enumerate() {
            let mut head = None;
            while let Some((_, title)) = headings.next_if(|(at, _)| *at <= i) {
                head = Some(title);
            }
            if let Some(title) = head {
                heads.push((items.len(), title));
            }
            if !filter || matches(&item.label, &text) {
                items.push(item);
            }
        }
        // A last group with no matches has no heading (not over the Create entry).
        let matched = items.len();
        heads.retain(|(at, _)| *at < matched);
        if self.on_create.is_some() && !typed.is_empty() && !exact {
            items
                .push(PopupMenuItem::new(CREATE, format!("Create “{typed}”")).icon(IconName::Plus));
        }
        let count = items.len();
        // A long list's rows are all one height: no headings there.
        let virtual_list = count > VIRTUAL_AFTER;
        let mut heads = heads.into_iter().peekable();
        let mut shown: Vec<Entry> = Vec::with_capacity(count);
        for (i, item) in items.into_iter().enumerate() {
            // A group with no matches has no entry at its index: the next group's heading wins.
            let mut head = None;
            while let Some((_, title)) = heads.next_if(|(at, _)| *at <= i) {
                head = Some(title);
            }
            if let Some(title) = head.filter(|_| !virtual_list) {
                shown.push(Entry::Heading(title));
            }
            shown.push(Entry::Item(item));
        }
        // A pick, or the Create entry's.
        let typed_text = text.clone();
        let on_select: Option<OnSelect> = match (self.on_select.clone(), self.on_create.clone()) {
            (None, None) => None,
            (select, create) => Some(Rc::new(move |pick: &MenuSelect, window, cx| {
                if pick.id.as_ref() == CREATE {
                    if let Some(f) = &create {
                        f(&typed, window, cx);
                    }
                } else if let Some(f) = &select {
                    // Every pick carries what's typed, from the keys or a press.
                    let pick = MenuSelect {
                        query: Some(typed_text.clone()),
                        ..pick.clone()
                    };
                    f(&pick, window, cx);
                }
            })),
        };
        let pickable: Rc<Vec<bool>> = Rc::new(
            shown
                .iter()
                .map(|e| as_item(e).is_some_and(|i| !i.disabled))
                .collect(),
        );
        // Where it opens: on the host's entry, else the current option (the checked one), else
        // the first match.
        let given = self.highlight.as_ref().and_then(|want| {
            shown
                .iter()
                .position(|e| as_item(e).is_some_and(|i| &i.id == want && !i.disabled))
        });
        let current = given.or_else(|| {
            shown
                .iter()
                .position(|e| as_item(e).is_some_and(|i| i.checked == Some(true) && !i.disabled))
        });
        let ids: Rc<Vec<SharedString>> = Rc::new(
            shown
                .iter()
                .map(|e| as_item(e).map(|i| i.id.clone()).unwrap_or_default())
                .collect(),
        );

        // Opening highlights the current option, a new query the first match; the pointer and
        // the keys move it from there.
        let (highlighted, requeried) = state.update(cx, |s, _| {
            let mut requeried = false;
            let mut placed = false;
            if s.seen.is_none() {
                s.seen = Some(text.clone());
                s.highlighted = current.or_else(|| step(&pickable, None, 1));
                placed = true;
            } else if s.seen.as_ref() != Some(&text) {
                s.seen = Some(text.clone());
                // A filtering field's new matches start from the first; a naming field's list
                // doesn't change, nor does its highlight.
                if filter {
                    s.highlighted = step(&pickable, None, 1);
                    requeried = true;
                    placed = true;
                }
            }
            if !placed {
                // The same entry as before, wherever it is now; if it's gone (or there was none,
                // the entries coming after it opened), the current option or the first.
                s.highlighted = s
                    .highlighted_id
                    .as_ref()
                    .and_then(|want| ids.iter().position(|id| id == want))
                    .filter(|i| pickable[*i])
                    .or_else(|| current.or_else(|| step(&pickable, None, 1)));
            }
            s.highlighted_id = s.highlighted.and_then(|i| ids.get(i).cloned());
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
                        s.highlighted_id = to.and_then(|i| ids.get(i).cloned());
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
                        query: None,
                    };
                    f(&pick, window, cx);
                }
            }
        });
        let (choose_shift, choose_secondary) = (choose.clone(), choose.clone());

        let theme = cx.theme().clone();
        let muted = self.colors.muted.unwrap_or(theme.muted_foreground);
        let empty = shown.is_empty();
        let set_highlight: crate::menu::SetHighlight =
            Rc::new(move |i, cx: &mut App| set(Some(i), cx));
        let list: gpui::AnyElement = if virtual_list {
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
                std::cell::RefCell::new(shown.into_iter().map(Some).collect()),
            );
            let (id, row, colors) = (self.id.clone(), self.row, self.colors);
            let scroll = state.read(cx).scroll.clone();
            // The list measures its rows on the first, twice a draw (laying out, then placing),
            // before it asks for the ones in view: those two get a text-only copy, leaving the
            // entry to be drawn, even when the one row in view is the first.
            let measures = Rc::new(std::cell::Cell::new(0));
            gpui::uniform_list("picker-rows", count, move |range, _, cx| {
                let taken: Vec<Entry> = {
                    let mut e = entries.borrow_mut();
                    let measure = range == (0..1) && measures.get() < MEASURES_PER_DRAW;
                    if range == (0..1) {
                        measures.set(measures.get() + 1);
                    }
                    if measure {
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
                    shown,
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
                    // On touch the field is a full target, as the rows are.
                    .when(self.row.touch, |d| d.min_h(theme.touch_target))
                    .debug_selector(|| "picker-field".into())
                    .when(self.search_icon.unwrap_or(filter), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .debug_selector(|| "picker-glass".into())
                                .child(Icon::new(IconName::Search).color(muted)),
                        )
                    })
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
                                .debug_selector(|| "picker-empty".into())
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
