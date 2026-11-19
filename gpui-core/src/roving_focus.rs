//! Roving focus: a list, a row of tiles or a tree whose rows share one Tab stop, with the arrow
//! keys moving between them (WAI-ARIA's "roving tabindex").
//!
//! Tab reaches the list once, on the row that last had the keyboard (else the selected row, else
//! the first), and the next Tab leaves it. Up and Down move between rows (Left and Right in a
//! horizontal row; any arrow among tiles), Home and End go to the ends, and Enter or Space
//! presses the row, as a click does. Each row is the host's [`Press`], given its words for
//! assistive tech; the list tells VoiceOver it's a list, a list box or a tree, and each row its
//! place in it.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::RovingFocus;
//! # use gpui_core::base::Press;
//! # struct People { open: Option<SharedString>, names: Vec<SharedString> }
//! # impl Render for People {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     let mut list = RovingFocus::new("people").accessibility_label("People");
//!     for name in &self.names {
//!         let n = name.clone();
//!         list = list.row(
//!             name.clone(),
//!             name.clone(),
//!             Press::new(name.clone())
//!                 .child(name.clone())
//!                 .on_click(cx.listener(move |this, _, _, cx| {
//!                     this.open = Some(n.clone());
//!                     cx.notify();
//!                 })),
//!         );
//!     }
//!     list
//! }
//! # }
//! ```
//!
//! A tree ([`RovingFocus::tree`]) is drawn by the host as the rows in view: a folded row's
//! children aren't rows. Each row says its depth and whether it's open
//! ([`RovingFocus::tree_row`]). Right opens a folded row (through the host's toggle) or goes to
//! its first child, and Left folds an open row or goes to its parent.
//!
//! A row that goes while it has the keyboard hands it to the row after it (or before it, at the
//! end), so the keyboard is never lost. The list doesn't scroll a row it moves to into view; a
//! host whose list scrolls does that.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder as _,
};

use crate::base::{Press, Selectable as _};

actions!(
    gpui_core_roving_focus,
    [
        /// The row above (a list, tiles or a tree).
        Up,
        /// The row below.
        Down,
        /// The tile before (a horizontal row or tiles); in a tree, folds the row or goes to its
        /// parent.
        Left,
        /// The tile after; in a tree, opens the row or goes to its first child.
        Right,
        /// The first row.
        First,
        /// The last row.
        Last
    ]
);

/// A roving list's key context: the arrows, Home and End move between its rows.
pub const CONTEXT: &str = "GpuiCoreRovingFocus";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("home", First, c),
        KeyBinding::new("end", Last, c),
    ]);
}

/// How the rows are laid out, which says which arrows move between them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RovingLayout {
    /// Rows one above another: Up and Down.
    #[default]
    Vertical,
    /// Side by side: Left and Right.
    Horizontal,
    /// Tiles that wrap: Left and Up go back, Right and Down on.
    Tiles,
    /// A tree's rows in view: Up and Down, and Left and Right fold, open and go up and down a
    /// level.
    Tree,
}

type OnToggle = Rc<dyn Fn(&SharedString, bool, &mut Window, &mut App)>;

struct Row {
    id: SharedString,
    label: SharedString,
    press: Press,
    /// Its depth in a tree, from 1.
    level: usize,
    /// In a tree, whether it's open (none for a leaf).
    expanded: Option<bool>,
}

enum Entry {
    Row(Box<Row>),
    /// Something drawn between the rows (a heading, a rule) that the keys pass over.
    Other(AnyElement),
}

/// Rows sharing one Tab stop, with the arrows moving between them (WAI-ARIA's roving
/// tabindex). Tab reaches them once, on the row that last had the keyboard (else the
/// [`selected`](Self::selected) one, else the first), and the next Tab leaves. Up and Down move
/// between rows ([`horizontal`](Self::horizontal): Left and Right; [`tiles`](Self::tiles): any
/// arrow), Home and End go to the ends, and Enter or Space presses the row. A
/// [`tree`](Self::tree) folds, opens and climbs with Left and Right. A row that goes while it has
/// the keyboard hands it to its neighbour. The list doesn't scroll a row into view.
#[derive(IntoElement)]
pub struct RovingFocus {
    id: ElementId,
    layout: RovingLayout,
    role: Option<Role>,
    label: Option<SharedString>,
    selected: Option<SharedString>,
    entries: Vec<Entry>,
    on_toggle: Option<OnToggle>,
    style: StyleRefinement,
}

impl RovingFocus {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            layout: RovingLayout::Vertical,
            role: None,
            label: None,
            selected: None,
            entries: Vec::new(),
            on_toggle: None,
            style: StyleRefinement::default(),
        }
    }

    /// Side by side, Left and Right moving between them.
    pub fn horizontal(mut self) -> Self {
        self.layout = RovingLayout::Horizontal;
        self
    }

    /// Tiles that wrap: every arrow moves, back or on.
    pub fn tiles(mut self) -> Self {
        self.layout = RovingLayout::Tiles;
        self
    }

    /// A tree: `toggle(id, open)` opens or folds a row, for Right and Left. Give its rows with
    /// [`tree_row`](Self::tree_row).
    pub fn tree(
        mut self,
        toggle: impl Fn(&SharedString, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.layout = RovingLayout::Tree;
        self.on_toggle = Some(Rc::new(toggle));
        self
    }

    /// What it is to assistive tech: a list (the default), or a list box for a list that
    /// chooses one (a page's navigation, with [`selected`](Self::selected)). A tree is a tree.
    /// Its rows follow: list items, options or tree items.
    pub fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The row that's chosen (the page open): the Tab stop until another row has had the
    /// keyboard, and selected to assistive tech in a list box or a tree.
    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    /// A row: its id (unique in the list), its words for assistive tech, and the [`Press`] the
    /// host draws it as (its look, its children, its `on_click`).
    pub fn row(
        mut self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        press: Press,
    ) -> Self {
        self.entries.push(Entry::Row(Box::new(Row {
            id: id.into(),
            label: label.into(),
            press,
            level: 1,
            expanded: None,
        })));
        self
    }

    /// A tree's row: as [`row`](Self::row), with its depth (1 at the top) and whether it's open
    /// (`None` for a row with no children).
    pub fn tree_row(
        mut self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        level: usize,
        expanded: Option<bool>,
        press: Press,
    ) -> Self {
        self.entries.push(Entry::Row(Box::new(Row {
            id: id.into(),
            label: label.into(),
            press,
            level: level.max(1),
            expanded,
        })));
        self
    }
}

/// Headings and rules between the rows: drawn in place, passed over by the keys.
impl ParentElement for RovingFocus {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.entries.extend(elements.into_iter().map(Entry::Other));
    }
}

/// The list's own box: its layout, gaps and padding.
impl Styled for RovingFocus {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a roving list keeps between frames.
#[derive(Default)]
struct State {
    /// A focus handle for each row, by id.
    handles: HashMap<SharedString, FocusHandle>,
    /// The rows' ids as last drawn.
    order: Vec<SharedString>,
    /// The row that last had the keyboard: the Tab stop.
    last: Option<SharedString>,
}

impl RenderOnce for RovingFocus {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let layout = self.layout;
        let rows: Vec<&Row> = self
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Row(r) => Some(r.as_ref()),
                Entry::Other(_) => None,
            })
            .collect();
        let ids: Vec<SharedString> = rows.iter().map(|r| r.id.clone()).collect();
        let levels: Rc<Vec<usize>> = Rc::new(rows.iter().map(|r| r.level).collect());
        let expanded: Rc<Vec<Option<bool>>> = Rc::new(rows.iter().map(|r| r.expanded).collect());

        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| State::default());
        let (handles, move_to, last) = state.update(cx, |s, cx| {
            for id in &ids {
                if !s.handles.contains_key(id) {
                    s.handles.insert(id.clone(), cx.focus_handle());
                }
            }
            let focused = ids
                .iter()
                .find(|id| s.handles.get(*id).is_some_and(|h| h.is_focused(window)))
                .cloned();
            let mut move_to = None;
            if let Some(f) = focused {
                s.last = Some(f);
            } else if let Some(gone) = s.last.clone().filter(|l| !ids.contains(l)) {
                // The row that last had the keyboard went: the one after it takes its place (or
                // the one before, at the end), and the keyboard with it if it had it.
                let had_it = s.handles.get(&gone).is_some_and(|h| h.is_focused(window));
                let at = s.order.iter().position(|i| *i == gone).unwrap_or(0);
                let after = s.order.iter().skip(at + 1).find(|i| ids.contains(i));
                let before = s.order.iter().take(at).rev().find(|i| ids.contains(i));
                let next = after.or(before).cloned();
                if had_it {
                    move_to = next.as_ref().map(|n| s.handles[n].clone());
                }
                s.last = next;
            }
            s.handles.retain(|k, _| ids.contains(k));
            s.order = ids.clone();
            let handles: Vec<FocusHandle> = ids.iter().map(|id| s.handles[id].clone()).collect();
            (handles, move_to, s.last.clone())
        });
        if let Some(h) = move_to {
            window.defer(cx, move |window, cx| window.focus(&h, cx));
        }
        let handles = Rc::new(handles);
        let ids = Rc::new(ids);
        // The one Tab stop: the row that last had the keyboard, else the chosen one, else the
        // first.
        let position =
            |id: &Option<SharedString>| id.as_ref().and_then(|id| ids.iter().position(|i| i == id));
        let stop = position(&last)
            .or_else(|| position(&self.selected))
            .or(if ids.is_empty() { None } else { Some(0) });

        // The row with the keyboard, else the Tab stop.
        let at = {
            let handles = handles.clone();
            move |window: &Window| handles.iter().position(|h| h.is_focused(window)).or(stop)
        };
        let focus = {
            let handles = handles.clone();
            move |i: usize, window: &mut Window, cx: &mut App| {
                if let Some(h) = handles.get(i) {
                    window.focus(h, cx);
                }
            }
        };
        let n = handles.len();
        let step = {
            let (at, focus) = (at.clone(), focus.clone());
            move |by: isize, window: &mut Window, cx: &mut App| {
                if let Some(i) = at(window) {
                    let to = i as isize + by;
                    if to >= 0 && (to as usize) < n {
                        focus(to as usize, window, cx);
                    }
                }
            }
        };
        let tree = layout == RovingLayout::Tree;
        let toggle = self.on_toggle.clone();
        // Left in a tree: fold an open row, else go to its parent.
        let tree_left = {
            let (at, focus, levels, expanded, ids, toggle) = (
                at.clone(),
                focus.clone(),
                levels.clone(),
                expanded.clone(),
                ids.clone(),
                toggle.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                let Some(i) = at(window) else { return };
                if expanded[i] == Some(true) {
                    if let Some(t) = &toggle {
                        t(&ids[i], false, window, cx);
                    }
                } else if let Some(p) = (0..i).rev().find(|j| levels[*j] < levels[i]) {
                    focus(p, window, cx);
                }
            }
        };
        // Right in a tree: open a folded row, else go to its first child.
        let tree_right = {
            let (at, focus, levels, expanded, ids, toggle) = (
                at.clone(),
                focus.clone(),
                levels.clone(),
                expanded.clone(),
                ids.clone(),
                toggle,
            );
            move |window: &mut Window, cx: &mut App| {
                let Some(i) = at(window) else { return };
                match expanded[i] {
                    Some(false) => {
                        if let Some(t) = &toggle {
                            t(&ids[i], true, window, cx);
                        }
                    }
                    Some(true) if levels.get(i + 1).is_some_and(|l| *l > levels[i]) => {
                        focus(i + 1, window, cx)
                    }
                    _ => {}
                }
            }
        };

        let role = self
            .role
            .unwrap_or(if tree { Role::Tree } else { Role::List });
        let row_role = match role {
            Role::ListBox => Role::ListBoxOption,
            Role::Tree => Role::TreeItem,
            _ => Role::ListItem,
        };
        let says_selected = matches!(role, Role::ListBox | Role::Tree);
        let selected = self.selected.clone();

        let (up, down, left, right) = (step.clone(), step.clone(), step.clone(), step.clone());
        let (first, last_row) = (focus.clone(), focus.clone());
        let mut list = div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .role(role)
            .when_some(self.label.clone(), |d, l| d.aria_label(l))
            .on_action(move |_: &Up, window, cx| match layout {
                RovingLayout::Horizontal => cx.propagate(),
                _ => up(-1, window, cx),
            })
            .on_action(move |_: &Down, window, cx| match layout {
                RovingLayout::Horizontal => cx.propagate(),
                _ => down(1, window, cx),
            })
            .on_action(move |_: &Left, window, cx| match layout {
                RovingLayout::Vertical => cx.propagate(),
                RovingLayout::Tree => tree_left(window, cx),
                _ => left(-1, window, cx),
            })
            .on_action(move |_: &Right, window, cx| match layout {
                RovingLayout::Vertical => cx.propagate(),
                RovingLayout::Tree => tree_right(window, cx),
                _ => right(1, window, cx),
            })
            .on_action(move |_: &First, window, cx| {
                if n > 0 {
                    first(0, window, cx)
                }
            })
            .on_action(move |_: &Last, window, cx| {
                if n > 0 {
                    last_row(n - 1, window, cx)
                }
            })
            .flex()
            .map(|d| match layout {
                RovingLayout::Horizontal => d.flex_row(),
                RovingLayout::Tiles => d.flex_row().flex_wrap(),
                _ => d.flex_col(),
            });
        gpui::Refineable::refine(list.style(), &self.style);

        let mut i = 0;
        list.children(self.entries.into_iter().map(|e| match e {
            Entry::Other(el) => el,
            Entry::Row(r) => {
                let handle = handles[i].clone();
                let is_stop = stop == Some(i);
                i += 1;
                let chosen = selected.as_ref() == Some(&r.id);
                r.press
                    .role(row_role)
                    .accessibility_label(r.label)
                    .track_focus(&handle)
                    .tab_stop(is_stop)
                    .when(says_selected, |p| p.selected(chosen))
                    .when(tree, |p| p.aria_level(r.level))
                    .when_some(r.expanded, |p, open| p.aria_expanded(open))
                    .into_any_element()
            }
        }))
    }
}
