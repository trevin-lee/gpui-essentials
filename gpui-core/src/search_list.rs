//! A searchable list: a search field above a list of rows the host draws, for a page with a
//! list beside a detail pane (a vault's logins, codes and cards).
//!
//! Typing filters the rows (every word of the query in an item's search text, case aside, as
//! [`Picker`](crate::base::Picker) does). Up and Down move a highlight through them while the field
//! keeps the keyboard, and Enter opens the highlighted row. A press opens a row too: a click or
//! a tap, once (a double press doesn't open it twice). The row that's open is the host's to say
//! ([`SearchList::selected`]); it's drawn selected, and the highlight goes to it. Escape clears
//! the search, and with nothing typed it's left to what's around the list (a sheet closing).
//!
//! Rows are one height (the host sets it), so only the rows in view are drawn, and a list of
//! thousands filters and moves as fast as a short one.
//!
//! ```no_run
//! # use gpui::{*, prelude::FluentBuilder as _};
//! # use gpui_core::SearchList;
//! # struct Vault { open: Option<SharedString>, items: Vec<(SharedString, SharedString)> }
//! # impl Render for Vault {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     SearchList::new("vault")
//!         .placeholder("Search the vault")
//!         .accessibility_label("Vault")
//!         .items(self.items.clone())
//!         .render_row(|id, _highlighted, _selected, _, _| div().child(id.clone()))
//!         .when_some(self.open.clone(), |l, id| l.selected(id))
//!         .on_open(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.open = Some(id.clone());
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AbsoluteLength, AnyElement, App, AppContext as _, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Pixels, RenderOnce, Role,
    ScrollStrategy, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    UniformListScrollHandle, Window, actions, div, prelude::FluentBuilder as _, rems, uniform_list,
};

use crate::base::{ActiveTheme as _, Icon, IconName, Input, InputState, Press, Selectable as _};

actions!(
    gpui_core_search_list,
    [
        /// The row before the highlighted one.
        Previous,
        /// The row after it.
        Next,
        /// Opens the highlighted row.
        Open,
        /// Clears the search (or, with nothing typed, leaves Escape to what's around the list).
        Clear,
    ]
);

const CONTEXT: &str = "GpuiCoreSearchList";
/// The list's field: its keys over the field's own (a one-line field's Up and Down would
/// otherwise go to its start and end, and Enter would submit it).
const FIELD: &str = "GpuiCoreSearchList > GpuiCoreInput";

pub(crate) fn init(cx: &mut App) {
    let (field, list) = (Some(FIELD), Some(CONTEXT));
    cx.bind_keys([
        KeyBinding::new("up", Previous, field),
        KeyBinding::new("down", Next, field),
        KeyBinding::new("enter", Open, field),
        // The same keys while a press has put the keyboard on the list itself.
        KeyBinding::new("up", Previous, list),
        KeyBinding::new("down", Next, list),
        KeyBinding::new("enter", Open, list),
        KeyBinding::new("escape", Clear, list),
    ]);
}

type RenderRow = Rc<dyn Fn(&SharedString, bool, bool, &mut Window, &mut App) -> AnyElement>;
type OnOpen = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
/// Moves the highlight to a row, by its index among those shown.
type SetHighlight = Rc<dyn Fn(usize, &mut App)>;

/// A search field above a list of rows the host draws. See the module's documentation for how it
/// behaves.
#[derive(IntoElement)]
pub struct SearchList {
    id: ElementId,
    items: Vec<(SharedString, SharedString)>,
    render_row: Option<RenderRow>,
    on_open: Option<OnOpen>,
    selected: Option<SharedString>,
    placeholder: SharedString,
    empty: SharedString,
    label: Option<SharedString>,
    row_height: Option<AbsoluteLength>,
    touch: bool,
    autofocus: bool,
    style: StyleRefinement,
}

impl SearchList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            render_row: None,
            on_open: None,
            selected: None,
            placeholder: "Search".into(),
            empty: "No matches".into(),
            label: None,
            row_height: None,
            touch: false,
            autofocus: false,
            style: StyleRefinement::default(),
        }
    }

    /// The rows, in order: each an id (what [`render_row`](Self::render_row) and
    /// [`on_open`](Self::on_open) are given) and the text a search looks in (its title, user
    /// name, site…). Ids are unique.
    pub fn items<I: Into<SharedString>, T: Into<SharedString>>(
        mut self,
        items: impl IntoIterator<Item = (I, T)>,
    ) -> Self {
        self.items
            .extend(items.into_iter().map(|(i, t)| (i.into(), t.into())));
        self
    }

    /// Draws a row's content from its id, whether it's highlighted (the keyboard's place in
    /// the list, shown while the list has the keyboard) and whether it's selected. The list
    /// draws the row around it: its height, the selected fill and the highlight's outline, and
    /// text in it cut short with an ellipsis.
    pub fn render_row<E: IntoElement>(
        mut self,
        f: impl Fn(&SharedString, bool, bool, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.render_row = Some(Rc::new(move |id, highlighted, selected, window, cx| {
            f(id, highlighted, selected, window, cx).into_any_element()
        }));
        self
    }

    /// Runs with a row's id when Enter or a press opens it. The host shows it and makes it the
    /// [`selected`](Self::selected) one.
    pub fn on_open(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }

    /// The row that's open (in the detail pane): drawn selected, and highlighted when it
    /// changes, when the list first shows and when the search is cleared.
    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    /// The field's placeholder ("Search" unless given), which is also its label to assistive
    /// tech. Taken when the list first shows.
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// What it shows when no row matches, or there are none ("No matches" unless given).
    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    /// What the list is, to assistive tech ("Vault").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Every row's height (32 CSS pixels unless given; at least 44 points with
    /// [`touch`](Self::touch)).
    pub fn row_height(mut self, height: impl Into<AbsoluteLength>) -> Self {
        self.row_height = Some(height.into());
        self
    }

    /// Rows and the field at least 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }

    /// Whether the field takes the keyboard when the list first shows (it doesn't unless told).
    pub fn autofocus(mut self, autofocus: bool) -> Self {
        self.autofocus = autofocus;
        self
    }
}

/// Its size and place, as a `div`'s: it fills its parent unless told otherwise, and the list
/// takes the height the field leaves.
impl Styled for SearchList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a list remembers between frames.
struct State {
    query: Entity<InputState>,
    /// The list's own focus, for the keys after a press (not a Tab stop: the field is).
    list_focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The query's words the highlight was placed for (`None` before the first frame).
    seen_query: Option<String>,
    /// The selected row the highlight was placed for.
    seen_selected: Option<SharedString>,
    /// The highlighted row, by id: it stays that row's while the host's rows come, go and
    /// move, so Enter opens the row that's shown highlighted.
    highlighted_id: Option<SharedString>,
    /// Its index among the rows shown last frame.
    highlighted: Option<usize>,
}

/// The query's words, lower-cased.
fn words(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Whether `text` has every word in it, case aside: [`Picker`](crate::base::Picker)'s rule.
fn matches(text: &str, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let text = text.to_lowercase();
    words.iter().all(|w| text.contains(w.as_str()))
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

impl RenderOnce for SearchList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let placeholder = self.placeholder.clone();
        let autofocus = self.autofocus;
        let state = window.use_keyed_state(self.id.clone(), cx, |window, cx| {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            if autofocus {
                let q = query.clone();
                window.defer(cx, move |window, cx| {
                    q.update(cx, |q, cx| q.focus(window, cx))
                });
            }
            State {
                query,
                list_focus: cx.focus_handle().tab_stop(false),
                scroll: UniformListScrollHandle::new(),
                seen_query: None,
                seen_selected: None,
                highlighted_id: None,
                highlighted: None,
            }
        });
        let (query, list_focus, scroll) = {
            let s = state.read(cx);
            (s.query.clone(), s.list_focus.clone(), s.scroll.clone())
        };
        let text = query.read(cx).value();
        let words = words(&text);
        let shown: Rc<Vec<SharedString>> = Rc::new(
            self.items
                .into_iter()
                .filter(|(_, t)| matches(t, &words))
                .map(|(id, _)| id)
                .collect(),
        );
        let count = shown.len();
        let selected = self.selected.clone();

        // Where the highlight is this frame: on the selected row when the list first shows,
        // when the search is cleared and when the host selects another; on the first match for
        // a new search; otherwise on the row it was on, wherever that is now.
        let highlighted = state.update(cx, |s, _| {
            let first = (count > 0).then_some(0);
            let at = |id: &Option<SharedString>| {
                id.as_ref()
                    .and_then(|id| shown.iter().position(|row| row == id))
            };
            let on_selected = at(&selected);
            let on_same = at(&s.highlighted_id);
            let joined = words.join(" ");
            let to = if s.seen_query.as_ref() != Some(&joined) {
                let first_frame = s.seen_query.is_none();
                s.seen_query = Some(joined);
                if first_frame || words.is_empty() {
                    on_selected.or(first)
                } else {
                    first
                }
            } else if s.seen_selected != selected {
                on_selected.or(on_same).or(first)
            } else {
                on_same.or(on_selected).or(first)
            };
            s.seen_selected = selected.clone();
            s.highlighted_id = to.map(|i| shown[i].clone());
            // Kept in view when it moves (a row moving under it included); the wheel's own
            // scrolling is left alone.
            if to != s.highlighted
                && let Some(i) = to
            {
                s.scroll.scroll_to_item(i, ScrollStrategy::Nearest);
            }
            s.highlighted = to;
            to
        });

        // Moves the highlight to row `to` and brings it into view.
        let set: SetHighlight = Rc::new({
            let (state, shown) = (state.clone(), shown.clone());
            move |to, cx| {
                state.update(cx, |s, cx| {
                    if s.highlighted != Some(to)
                        && let Some(id) = shown.get(to)
                    {
                        s.highlighted = Some(to);
                        s.highlighted_id = Some(id.clone());
                        s.scroll.scroll_to_item(to, ScrollStrategy::Nearest);
                        cx.notify();
                    }
                })
            }
        });
        let mover = |down: bool| {
            let (state, set) = (state.clone(), set.clone());
            move |cx: &mut App| {
                if count == 0 {
                    return;
                }
                let to = match (state.read(cx).highlighted, down) {
                    (None, true) => 0,
                    (None, false) => count - 1,
                    (Some(i), true) => (i + 1).min(count - 1),
                    (Some(i), false) => i.saturating_sub(1),
                };
                set(to, cx);
            }
        };
        let (prev, next) = (mover(false), mover(true));
        let open = {
            let (state, shown, on_open) = (state.clone(), shown.clone(), self.on_open.clone());
            move |window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let (Some(id), Some(f)) = (at.and_then(|i| shown.get(i)), &on_open) {
                    f(id, window, cx);
                }
            }
        };
        let clear = {
            let query = query.clone();
            move |window: &mut Window, cx: &mut App| {
                if query.read(cx).value().is_empty() {
                    cx.propagate();
                } else {
                    query.update(cx, |q, cx| q.set_value(String::new(), window, cx));
                }
            }
        };

        let theme = cx.theme().clone();
        let field_focused = query.read(cx).is_focused(window);
        let keyboard = field_focused || list_focus.is_focused(window);
        let rem = window.rem_size();
        let row_px: Pixels = self
            .row_height
            .map(|h| h.to_pixels(rem))
            .unwrap_or_else(|| css(32.).to_pixels(rem));
        let row_px = if self.touch {
            row_px.max(theme.touch_target)
        } else {
            row_px
        };

        let list: AnyElement = if count == 0 {
            div()
                .px(css(8.))
                .py(css(6.))
                .text_size(theme.text.sm)
                .text_color(theme.muted_foreground)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .debug_selector(|| "search-list-empty".into())
                .child(self.empty)
                .into_any_element()
        } else {
            let list_id = Arc::new(self.id.clone());
            let render_row = self.render_row.clone();
            let on_open = self.on_open.clone();
            let theme = theme.clone();
            let query = query.clone();
            let list_focus = list_focus.clone();
            // Only the rows in view are made (and the first, which the list measures).
            uniform_list("search-list-rows", count, move |range, window, cx| {
                range
                    .map(|i| {
                        let id = shown[i].clone();
                        let on = keyboard && highlighted == Some(i);
                        let selected = selected.as_ref() == Some(&id);
                        let content = render_row
                            .as_ref()
                            .map(|f| f(&id, on, selected, window, cx));
                        let (set, on_open, query, list_focus) = (
                            set.clone(),
                            on_open.clone(),
                            query.clone(),
                            list_focus.clone(),
                        );
                        Press::new(ElementId::NamedChild(list_id.clone(), id.clone()))
                            .role(Role::ListBoxOption)
                            .selected(selected)
                            .aria_position_in_set(i + 1)
                            .aria_size_of_set(count)
                            .tab_stop(false)
                            .focus_on_press(false)
                            .focus_ring(false)
                            .debug_selector({
                                let id = id.clone();
                                move || format!("search-row-{id}")
                            })
                            .flex()
                            .flex_none()
                            .items_center()
                            .w_full()
                            .min_w_0()
                            .h(row_px)
                            .px(css(8.))
                            .rounded(theme.radius_sm())
                            // Always there, so the outline doesn't move the content.
                            .border_1()
                            .when(on, |r| r.border_color(theme.ring))
                            .when(selected, |r| {
                                r.bg(theme.accent).text_color(theme.accent_foreground)
                            })
                            .text_size(theme.text.sm)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .when_some(content, |d, c| d.child(c)),
                            )
                            .on_click(move |e, window, cx| {
                                // A double press is the second of two: the row's open already.
                                if e.click_count() > 1 {
                                    return;
                                }
                                set(i, cx);
                                // The keys go on from here: in the field if it had them, else
                                // in the list (not the field, which would raise the on-screen
                                // keyboard on a phone).
                                if !query.read(cx).is_focused(window) {
                                    window.focus(&list_focus, cx);
                                }
                                if let Some(f) = &on_open {
                                    f(&id, window, cx);
                                }
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .track_scroll(&scroll)
            .flex_1()
            .min_h_0()
            .w_full()
            .px(css(4.))
            .into_any_element()
        };

        let field_h = if self.touch {
            theme.touch_target.into()
        } else {
            gpui::DefiniteLength::from(css(36.))
        };
        let mut root = div()
            .id(self.id)
            .key_context(CONTEXT)
            .on_action(move |_: &Previous, _, cx| prev(cx))
            .on_action(move |_: &Next, _, cx| next(cx))
            .on_action(move |_: &Open, w, cx| open(w, cx))
            .on_action(move |_: &Clear, w, cx| clear(w, cx))
            .flex()
            .flex_col()
            .gap(css(6.))
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                // The field, with a magnifying glass laid over its padding.
                div()
                    .relative()
                    .flex_none()
                    .w_full()
                    .px(css(4.))
                    .debug_selector(|| "search-list-field".into())
                    .child(
                        Input::new(&query)
                            .accessibility_label(self.placeholder)
                            .min_h(field_h)
                            .pl(css(32.)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(css(14.))
                            .flex()
                            .items_center()
                            .child(Icon::new(IconName::Search).color(theme.muted_foreground)),
                    ),
            )
            .child(
                div()
                    .id("search-list")
                    .role(Role::ListBox)
                    .when_some(self.label, |d, l| d.aria_label(l))
                    .track_focus(&list_focus)
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(list),
            );
        gpui::Refineable::refine(root.style(), &self.style);
        root
    }
}
