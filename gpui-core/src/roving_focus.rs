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
//! A row can hold controls of its own (a "⋯", a download), named with
//! [`RovingFocus::row_actions`]: they aren't Tab stops; Right from the row goes into them and on
//! through them, Left comes back, Up and Down go to the neighbouring rows, and Escape returns to
//! the row. Shift-F10 or the context-menu key opens a row's menu. VoiceOver hears how many
//! actions a row has and the way in.
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
        Last,
        /// From a row's action, back to its row (Escape).
        Back,
        /// Opens the row's menu action, if it has one (Shift-F10, the context-menu key).
        OpenMenu
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
        KeyBinding::new("escape", Back, c),
        KeyBinding::new("shift-f10", OpenMenu, c),
        KeyBinding::new("menu", OpenMenu, c),
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
    /// Controls inside it (a "⋯", a download) that Right reaches.
    actions: Vec<RowAction>,
}

/// A control inside a row (a "⋯", a download) that the keys reach from the row: Right goes
/// into the row's actions and through them, Left comes back. The host draws it, tracking
/// [`focus`](Self::new)'s handle with Tab off (`Press::track_focus(&handle).tab_stop(false)`), so Tab
/// still enters and leaves the list once.
#[derive(Clone)]
pub struct RowAction {
    focus: FocusHandle,
    menu: bool,
}

impl RowAction {
    /// The control that tracks `focus`.
    pub fn new(focus: &FocusHandle) -> Self {
        Self {
            focus: focus.clone(),
            menu: false,
        }
    }

    /// It opens a menu (a DropdownMenu's trigger): Shift-F10 or the context-menu key on the
    /// row goes to it and opens it, as Down would.
    pub fn menu(mut self) -> Self {
        self.menu = true;
        self
    }
}

/// What a row with actions says besides its words: how many, and the way in.
fn describe(actions: usize) -> Option<SharedString> {
    match actions {
        0 => None,
        1 => Some("1 action, press Right".into()),
        n => Some(format!("{n} actions, press Right").into()),
    }
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
    /// A row to give the keyboard to, once (see [`focus`](Self::focus)).
    focus: Option<SharedString>,
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
            focus: None,
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

    /// Gives row `id` the keyboard, and makes it the Tab stop and the row Tab comes back to: after
    /// an inline rename ends, say, when the row's field goes. It's done once for a request: drawn
    /// again with the same id, the list leaves the keyboard where it is; a frame without it, the
    /// next request counts again. Nothing, for an id that isn't a row.
    pub fn focus(mut self, id: impl Into<SharedString>) -> Self {
        self.focus = Some(id.into());
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
            actions: Vec::new(),
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
            actions: Vec::new(),
        })));
        self
    }
}

impl RovingFocus {
    /// The controls inside row `id` (its "⋯", a download), in order: Right from the row goes
    /// to the first and on through them, Left back and from the first to the row, Up and Down
    /// to the neighbouring rows, Escape back to the row. In a tree, Right opens a folded row
    /// first, as ever, and goes into an open row's or a leaf's actions. Shift-F10 or the
    /// context-menu key opens the first that's a [`menu`](RowAction::menu). VoiceOver hears
    /// how many a row has.
    pub fn row_actions(
        mut self,
        id: impl Into<SharedString>,
        actions: impl IntoIterator<Item = RowAction>,
    ) -> Self {
        let id = id.into();
        if let Some(Entry::Row(r)) = self
            .entries
            .iter_mut()
            .find(|e| matches!(e, Entry::Row(r) if r.id == id))
        {
            r.actions = actions.into_iter().collect();
        }
        self
    }
}

/// The keys' way through the rows and their actions.
struct Nav {
    layout: RovingLayout,
    ids: Vec<SharedString>,
    handles: Vec<FocusHandle>,
    actions: Vec<Vec<RowAction>>,
    levels: Vec<usize>,
    expanded: Vec<Option<bool>>,
    toggle: Option<OnToggle>,
    /// The Tab stop: where the keys start when nothing here has the keyboard.
    stop: Option<usize>,
}

impl Nav {
    /// The row with the keyboard, and which of its actions if one has it; else the Tab stop.
    fn at(&self, window: &Window) -> Option<(usize, Option<usize>)> {
        if let Some(i) = self.handles.iter().position(|h| h.is_focused(window)) {
            return Some((i, None));
        }
        for (i, acts) in self.actions.iter().enumerate() {
            if let Some(k) = acts.iter().position(|a| a.focus.is_focused(window)) {
                return Some((i, Some(k)));
            }
        }
        self.stop.map(|i| (i, None))
    }

    fn row(&self, i: usize, window: &mut Window, cx: &mut App) {
        if let Some(h) = self.handles.get(i) {
            window.focus(h, cx);
        }
    }

    fn action(&self, i: usize, k: usize, window: &mut Window, cx: &mut App) {
        if let Some(a) = self.actions.get(i).and_then(|a| a.get(k)) {
            window.focus(&a.focus, cx);
        }
    }

    /// Up (-1) or Down (1): the neighbouring row, from a row or one of its actions; from an
    /// action at the end, its own row.
    fn up_down(&self, by: isize, window: &mut Window, cx: &mut App) -> bool {
        if self.layout == RovingLayout::Horizontal {
            return false;
        }
        let Some((i, k)) = self.at(window) else {
            return true;
        };
        let to = i as isize + by;
        if to >= 0 && (to as usize) < self.handles.len() {
            self.row(to as usize, window, cx);
        } else if k.is_some() {
            self.row(i, window, cx);
        }
        true
    }

    fn left(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some((i, k)) = self.at(window) else {
            return true;
        };
        match (k, self.layout) {
            (Some(0), _) => self.row(i, window, cx),
            (Some(k), _) => self.action(i, k - 1, window, cx),
            (None, RovingLayout::Vertical) => return false,
            (None, RovingLayout::Tree) => {
                // Fold an open row, else go to its parent.
                if self.expanded[i] == Some(true) {
                    if let Some(t) = &self.toggle {
                        t(&self.ids[i], false, window, cx);
                    }
                } else if let Some(p) = (0..i).rev().find(|j| self.levels[*j] < self.levels[i]) {
                    self.row(p, window, cx);
                }
            }
            (None, _) => {
                if i > 0 {
                    self.row(i - 1, window, cx);
                }
            }
        }
        true
    }

    fn right(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some((i, k)) = self.at(window) else {
            return true;
        };
        let has_actions = !self.actions[i].is_empty();
        match (k, self.layout) {
            (Some(k), _) => {
                if k + 1 < self.actions[i].len() {
                    self.action(i, k + 1, window, cx);
                }
            }
            (None, RovingLayout::Vertical) => {
                if !has_actions {
                    return false;
                }
                self.action(i, 0, window, cx);
            }
            (None, RovingLayout::Tree) => match self.expanded[i] {
                // A folded row opens first.
                Some(false) => {
                    if let Some(t) = &self.toggle {
                        t(&self.ids[i], true, window, cx);
                    }
                }
                _ if has_actions => self.action(i, 0, window, cx),
                Some(true) if self.levels.get(i + 1).is_some_and(|l| *l > self.levels[i]) => {
                    self.row(i + 1, window, cx)
                }
                _ => {}
            },
            (None, _) => {
                if i + 1 < self.handles.len() {
                    self.row(i + 1, window, cx);
                }
            }
        }
        true
    }

    /// Escape from an action: back to its row.
    fn back(&self, window: &mut Window, cx: &mut App) -> bool {
        match self.at(window) {
            Some((i, Some(_))) => {
                self.row(i, window, cx);
                true
            }
            _ => false,
        }
    }

    /// Shift-F10: to the row's menu action, opened as Down opens it.
    fn open_menu(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some((i, _)) = self.at(window) else {
            return false;
        };
        let Some(menu) = self.actions[i].iter().find(|a| a.menu) else {
            return false;
        };
        window.focus(&menu.focus, cx);
        if let Ok(down) = gpui::Keystroke::parse("down") {
            window.dispatch_keystroke(down, cx);
        }
        true
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
    /// The focus request last done, so a request drawn again isn't done again.
    focused_for: Option<SharedString>,
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
        let levels: Vec<usize> = rows.iter().map(|r| r.level).collect();
        let expanded: Vec<Option<bool>> = rows.iter().map(|r| r.expanded).collect();
        let actions: Vec<Vec<RowAction>> = rows.iter().map(|r| r.actions.clone()).collect();
        // A row whose action has the keyboard is where Tab comes back to.
        let action_focused: Option<SharedString> = rows
            .iter()
            .find(|r| r.actions.iter().any(|a| a.focus.is_focused(window)))
            .map(|r| r.id.clone());

        let request = self.focus.clone();
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
                .cloned()
                .or(action_focused);
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
            // A request to give a row the keyboard: done once, when it's new and the row is here.
            if let Some(want) = request
                .as_ref()
                .filter(|w| ids.contains(w) && s.focused_for.as_ref() != Some(w))
            {
                s.last = Some(want.clone());
                move_to = Some(s.handles[want].clone());
            }
            s.focused_for = request.clone();
            s.handles.retain(|k, _| ids.contains(k));
            s.order = ids.clone();
            let handles: Vec<FocusHandle> = ids.iter().map(|id| s.handles[id].clone()).collect();
            (handles, move_to, s.last.clone())
        });
        if let Some(h) = move_to {
            window.defer(cx, move |window, cx| window.focus(&h, cx));
        }
        let position =
            |id: &Option<SharedString>| id.as_ref().and_then(|id| ids.iter().position(|i| i == id));
        // The one Tab stop: the row that last had the keyboard, else the chosen one, else the
        // first.
        let stop = position(&last)
            .or_else(|| position(&self.selected))
            .or(if ids.is_empty() { None } else { Some(0) });
        let tree = layout == RovingLayout::Tree;
        let n = handles.len();
        let nav = Rc::new(Nav {
            layout,
            ids: ids.clone(),
            handles: handles.clone(),
            actions: actions.clone(),
            levels,
            expanded,
            toggle: self.on_toggle.clone(),
            stop,
        });

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

        let on = |f: fn(&Nav, &mut Window, &mut App) -> bool| {
            let nav = nav.clone();
            move |window: &mut Window, cx: &mut App| {
                if !f(&nav, window, cx) {
                    cx.propagate();
                }
            }
        };
        let (up, down, left, right, back, menu) = (
            on(|n, w, cx| n.up_down(-1, w, cx)),
            on(|n, w, cx| n.up_down(1, w, cx)),
            on(Nav::left),
            on(Nav::right),
            on(Nav::back),
            on(Nav::open_menu),
        );
        let (first, last_row) = (nav.clone(), nav.clone());
        let mut list = div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .role(role)
            .when_some(self.label.clone(), |d, l| d.aria_label(l))
            .on_action(move |_: &Up, window, cx| up(window, cx))
            .on_action(move |_: &Down, window, cx| down(window, cx))
            .on_action(move |_: &Left, window, cx| left(window, cx))
            .on_action(move |_: &Right, window, cx| right(window, cx))
            .on_action(move |_: &Back, window, cx| back(window, cx))
            .on_action(move |_: &OpenMenu, window, cx| menu(window, cx))
            .on_action(move |_: &First, window, cx| {
                if n > 0 {
                    first.row(0, window, cx)
                }
            })
            .on_action(move |_: &Last, window, cx| {
                if n > 0 {
                    last_row.row(n - 1, window, cx)
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
                    .when_some(describe(r.actions.len()), |p, d| p.aria_description(d))
                    .into_any_element()
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_with_actions_says_how_many_and_the_way_in() {
        assert_eq!(describe(0), None);
        assert_eq!(describe(1).as_deref(), Some("1 action, press Right"));
        assert_eq!(describe(2).as_deref(), Some("2 actions, press Right"));
    }
}
