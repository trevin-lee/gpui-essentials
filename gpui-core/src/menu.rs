//! A menu: a list of entries to pick from, with the keyboard or the pointer. It's drawn as the
//! content of a [`Popover`](crate::Popover) (a button's menu, a column's header menu), or in
//! place.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, IconName, PopupMenu, PopupMenuItem, Popover};
//! # struct Panel { open: bool, wrap: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     let close = cx.listener(|this: &mut Panel, _: &(), _, cx| {
//!         this.open = false;
//!         cx.notify();
//!     });
//!     Popover::new("more")
//!         .open(self.open)
//!         .on_dismiss(cx.listener(|this, _, _, cx| {
//!             this.open = false;
//!             cx.notify();
//!         }))
//!         .trigger(Button::new("more-button").label("More").on_click(cx.listener(
//!             |this, _, _, cx| {
//!                 this.open = !this.open;
//!                 cx.notify();
//!             },
//!         )))
//!         .p(px(4.))
//!         .child(
//!             PopupMenu::new("more-menu")
//!                 .item(PopupMenuItem::new("copy", "Copy").icon(IconName::Copy).shortcut("⌘C"))
//!                 .separator()
//!                 .item(PopupMenuItem::new("wrap", "Wrap lines").checked(self.wrap))
//!                 .item(PopupMenuItem::new("move", "Move to").submenu(
//!                     PopupMenu::new("move-menu")
//!                         .item(PopupMenuItem::new("inbox", "Inbox"))
//!                         .item(PopupMenuItem::new("archive", "Archive")),
//!                 ))
//!                 .on_select(move |pick, window, cx| {
//!                     // `pick.id` is the entry's: "copy", "wrap", or a submenu's "inbox" or
//!                     // "archive"; `pick.modifiers` what's held.
//!                     close(&(), window, cx)
//!                 }),
//!         )
//! }
//! # }
//! ```

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, ContentMask, Element, ElementId, FocusHandle,
    GlobalElementId, Hsla, InspectorElementId, InteractiveElement as _, IntoElement, KeyBinding,
    LayoutId, Modifiers, ParentElement as _, Pixels, Point, Rems, RenderOnce, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, Window, actions, div, point,
    prelude::FluentBuilder as _, px, rems,
};

use crate::icon::{Icon, IconName};
use crate::popover::ClaimArea;
use crate::press::Press;
use crate::size::{Disableable as _, Selectable as _};
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_menu,
    [
        Previous,
        Next,
        First,
        Last,
        Choose,
        ChooseShift,
        ChooseSecondary,
        OpenSubmenu,
        CloseSubmenu
    ]
);

const CONTEXT: &str = "GpuiCoreMenu";
/// A submenu's context: the menu's, and `submenu`.
const SUBMENU_CONTEXT: &str = "GpuiCoreMenu submenu";
const IN_SUBMENU: &str = "GpuiCoreMenu && submenu";

/// How long the pointer rests on an entry before its submenu opens (or the open one closes, on
/// an entry without one): long enough to cross other entries on the way into a submenu.
pub const SUBMENU_DELAY: Duration = Duration::from_millis(200);

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("down", Next, c),
        KeyBinding::new("home", First, c),
        KeyBinding::new("end", Last, c),
        KeyBinding::new("enter", Choose, c),
        KeyBinding::new("space", Choose, c),
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
        KeyBinding::new("right", OpenSubmenu, c),
        // Only in a submenu, so in a popover's own menu Escape still closes the popover.
        KeyBinding::new("left", CloseSubmenu, Some(IN_SUBMENU)),
        KeyBinding::new("escape", CloseSubmenu, Some(IN_SUBMENU)),
    ]);
}

/// A pick: the entry's id, and the modifiers held (Shift-Enter or a Shift-click to add rather
/// than replace, say). From the keyboard, Shift-Enter and Cmd-Enter (Ctrl-Enter off the Mac)
/// come with those.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSelect {
    pub id: SharedString,
    pub modifiers: Modifiers,
    /// What's typed in a [`Picker`](crate::Picker)'s field as it's picked (none from a
    /// [`PopupMenu`]): a new property's name, with its type picked below it.
    pub query: Option<SharedString>,
}

/// The modifiers a keyboard pick comes with.
pub(crate) fn shift() -> Modifiers {
    Modifiers::shift()
}

pub(crate) fn secondary(cx: &App) -> Modifiers {
    Modifiers::secondary_key(cx)
}

/// One entry: a label, and optionally an icon, a check, a shortcut's text and a chip the host
/// draws (a select option's colour).
pub struct PopupMenuItem {
    pub(crate) id: SharedString,
    pub(crate) label: SharedString,
    icon: Option<IconName>,
    pub(crate) checked: Option<bool>,
    pub(crate) disabled: bool,
    shortcut: Option<SharedString>,
    chip: Option<AnyElement>,
    label_element: Option<AnyElement>,
    submenu: Option<SubmenuSource>,
    /// Whether it opens a submenu (kept by `text_copy`, which leaves the submenu out).
    opens: bool,
}

impl PopupMenuItem {
    /// `id` is what [`PopupMenu::on_select`] gets when it's picked.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            checked: None,
            disabled: false,
            shortcut: None,
            chip: None,
            label_element: None,
            submenu: None,
            opens: false,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A toggle or one option of several: drawn with a check while `checked`, and told to
    /// assistive tech as a checkable entry.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Shown and skipped: the keys pass over it and a press does nothing.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// A shortcut's text at the end ("⌘C"), for show: the menu doesn't bind it.
    pub fn shortcut(mut self, text: impl Into<SharedString>) -> Self {
        self.shortcut = Some(text.into());
        self
    }

    /// The entry without what the host draws (its chip, its drawn label): a stand-in a long
    /// list measures its rows with, leaving the entry itself to be drawn.
    pub(crate) fn text_copy(&self) -> Self {
        Self {
            id: self.id.clone(),
            label: self.label.clone(),
            icon: self.icon,
            checked: self.checked,
            disabled: self.disabled,
            shortcut: self.shortcut.clone(),
            chip: None,
            label_element: None,
            submenu: None,
            opens: self.opens,
        }
    }

    /// Something the host draws in the label's place: a select option's coloured pill, say.
    /// The label still names the entry to assistive tech and to a picker's filter.
    pub fn label_element(mut self, element: impl IntoElement) -> Self {
        self.label_element = Some(element.into_any_element());
        self
    }

    /// Something the host draws before the label: a colour's swatch, an avatar.
    pub fn chip(mut self, chip: impl IntoElement) -> Self {
        self.chip = Some(chip.into_any_element());
        self
    }

    /// Opens `menu` beside it rather than being picked: drawn with a chevron, it opens on a
    /// press, Enter, Space or Right, or when the pointer rests on it ([`SUBMENU_DELAY`]); Left
    /// or Escape closes it again. Its entries are drawn in this menu's row style and colours,
    /// and its picks go to its own [`PopupMenu::on_select`], or else to this menu's.
    pub fn submenu(mut self, menu: PopupMenu) -> Self {
        self.submenu = Some(SubmenuSource::Built(Box::new(menu)));
        self.opens = true;
        self
    }

    /// A [`submenu`](Self::submenu) made by `build` only when it opens: for a long list where
    /// each entry has one (an option's colour and actions, say).
    pub fn submenu_with(mut self, build: impl Fn() -> PopupMenu + 'static) -> Self {
        self.submenu = Some(SubmenuSource::Lazy(Rc::new(build)));
        self.opens = true;
        self
    }
}

/// A submenu, made or to be made when it opens.
enum SubmenuSource {
    Built(Box<PopupMenu>),
    Lazy(Rc<dyn Fn() -> PopupMenu>),
}

impl SubmenuSource {
    fn make(self) -> PopupMenu {
        match self {
            SubmenuSource::Built(menu) => *menu,
            SubmenuSource::Lazy(build) => build(),
        }
    }
}

pub(crate) enum Entry {
    Item(PopupMenuItem),
    Separator,
    /// A section's title: shown, never highlighted or picked.
    Heading(SharedString),
}

/// A row's height and text size, and the touch size over them.
#[derive(Clone, Copy, Default)]
pub(crate) struct RowStyle {
    pub touch: bool,
    pub height: Option<gpui::AbsoluteLength>,
    pub text_size: Option<gpui::AbsoluteLength>,
    /// Headings' text size.
    pub heading_size: Option<gpui::AbsoluteLength>,
    /// The check in a column before the label, every entry keeping room for it, rather than
    /// after the label.
    pub check_leading: bool,
}

/// A menu's colours, each over the theme's when given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopupMenuColors {
    /// The highlighted entry's fill (the theme's accent).
    pub highlight: Option<Hsla>,
    /// The highlighted entry's text (the theme's accent foreground).
    pub highlight_text: Option<Hsla>,
    /// Shortcuts, icons and disabled entries (the theme's muted foreground).
    pub muted: Option<Hsla>,
    /// Separators (the theme's border).
    pub separator: Option<Hsla>,
    /// A checked entry's check (the entry's text colour).
    pub check: Option<Hsla>,
    /// Section headings (the muted colour).
    pub heading: Option<Hsla>,
}

/// Moves the highlight to an index, or clears it.
pub(crate) type MoveHighlight = Rc<dyn Fn(Option<usize>, &mut App)>;

/// Highlights the entry at an index.
pub(crate) type SetHighlight = Rc<dyn Fn(usize, &mut App)>;

pub(crate) type OnSelect = Rc<dyn Fn(&MenuSelect, &mut Window, &mut App)>;

/// Told the highlighted entry's id when the keys or the pointer move the highlight.
pub(crate) type OnHighlight = Rc<dyn Fn(Option<&SharedString>, &mut App)>;

/// A list of [`PopupMenuItem`]s and separators. Up and Down move a highlight over the entries that
/// can be picked (the pointer moves it too), Home and End go to the ends, Enter or Space picks,
/// and a press picks. It takes the keyboard when it's first drawn, so in a
/// [`Popover`](crate::Popover) the keys work at once and Escape closes the popover.
#[derive(IntoElement)]
pub struct PopupMenu {
    id: ElementId,
    entries: Vec<Entry>,
    pub(crate) on_select: Option<OnSelect>,
    on_highlight: Option<OnHighlight>,
    highlight: Option<SharedString>,
    row: RowStyle,
    take_focus: bool,
    colors: PopupMenuColors,
    /// The entry whose submenu is open when it opens.
    open_submenu: Option<SharedString>,
    /// Set when it's drawn as another menu's submenu: how it closes.
    nested: Option<CloseFn>,
    /// Opened by the pointer: nothing highlighted until the keys or the pointer move.
    start_blank: bool,
}

type CloseFn = Rc<dyn Fn(&mut Window, &mut App)>;

impl PopupMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            entries: Vec::new(),
            on_select: None,
            on_highlight: None,
            highlight: None,
            row: RowStyle::default(),
            take_focus: true,
            colors: PopupMenuColors::default(),
            open_submenu: None,
            nested: None,
            start_blank: false,
        }
    }

    /// Opens with entry `id`'s submenu open and the keyboard in it (its first entry
    /// highlighted), and `id` highlighted here: to bring a keyboard user back where they were.
    pub fn open_submenu(mut self, id: impl Into<SharedString>) -> Self {
        self.open_submenu = Some(id.into());
        self
    }

    pub fn item(mut self, item: PopupMenuItem) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = PopupMenuItem>) -> Self {
        self.entries.extend(items.into_iter().map(Entry::Item));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }

    /// A section's title, between entries: shown muted, passed over by the keys and the pointer.
    pub fn heading(mut self, title: impl Into<SharedString>) -> Self {
        self.entries.push(Entry::Heading(title.into()));
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

    /// The headings' text size (the theme's extra-small size unless given).
    pub fn heading_size(mut self, size: impl Into<gpui::AbsoluteLength>) -> Self {
        self.row.heading_size = Some(size.into());
        self
    }

    /// Puts the check in a column before each label, as Notion's menus do, every entry keeping
    /// room for it so the labels line up; by default it follows the label.
    pub fn check_leading(mut self, leading: bool) -> Self {
        self.row.check_leading = leading;
        self
    }

    /// Runs with the pick: the entry's id and the modifiers held. A host whose menu is in a
    /// popover closes it here.
    pub fn on_select(mut self, f: impl Fn(&MenuSelect, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The entry highlighted when it opens (the first that can be picked unless given): the
    /// current option, say.
    pub fn highlight(mut self, id: impl Into<SharedString>) -> Self {
        self.highlight = Some(id.into());
        self
    }

    /// Told the highlighted entry's id when the keys or the pointer move the highlight (not
    /// when it opens: that's [`highlight`](Self::highlight)'s, or the first entry).
    pub fn on_highlight(mut self, f: impl Fn(Option<&SharedString>, &mut App) + 'static) -> Self {
        self.on_highlight = Some(Rc::new(f));
        self
    }

    /// Entries 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.row.touch = touch;
        self
    }

    /// Whether it takes the keyboard when first drawn (on by default): off for a menu drawn in
    /// place that shouldn't take it from the page.
    pub fn take_focus(mut self, take: bool) -> Self {
        self.take_focus = take;
        self
    }

    pub fn colors(mut self, colors: PopupMenuColors) -> Self {
        self.colors = colors;
        self
    }
}

/// What a menu remembers between frames.
struct MenuState {
    focus: FocusHandle,
    /// The highlighted entry's index this frame, worked out from its id.
    highlighted: Option<usize>,
    /// The highlighted entry, by its id: it stays that entry's while entries come, go and move,
    /// so Enter picks the entry that's shown highlighted.
    highlighted_id: Option<SharedString>,
    focused_once: bool,
    /// Whether it has set its first highlight.
    opened: bool,
    /// The entry whose submenu is open, by its id: it stays that entry's while entries come,
    /// go and move.
    open_sub: Option<SharedString>,
    /// Whether the pointer opened it (it then opens with nothing highlighted).
    sub_by_pointer: bool,
    /// Counts the pointer's moves, so a rest that's been moved on from does nothing.
    rest: u64,
}

/// The entry at `at` (clamped to the menu) if it can be picked, else the nearest before it,
/// else after it.
fn nearest(pickable: &[bool], at: Option<usize>) -> Option<usize> {
    let at = at?.min(pickable.len().checked_sub(1)?);
    if pickable[at] {
        return Some(at);
    }
    (0..at)
        .rev()
        .find(|i| pickable[*i])
        .or_else(|| (at..pickable.len()).find(|i| pickable[*i]))
}

/// The next entry from `from` in `step`'s direction that can be picked, wrapping.
pub(crate) fn step(pickable: &[bool], from: Option<usize>, step: isize) -> Option<usize> {
    let n = pickable.len() as isize;
    if !pickable.iter().any(|p| *p) {
        return None;
    }
    let mut i = match from {
        Some(i) => i as isize,
        None if step > 0 => -1,
        None => n,
    };
    loop {
        i = (i + step).rem_euclid(n);
        if pickable[i as usize] {
            return Some(i as usize);
        }
    }
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

impl RenderOnce for PopupMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| MenuState {
            focus: cx.focus_handle(),
            highlighted: None,
            highlighted_id: None,
            focused_once: false,
            opened: false,
            open_sub: None,
            sub_by_pointer: false,
            rest: 0,
        });
        let (focus, focused_once) = {
            let s = state.read(cx);
            (s.focus.clone(), s.focused_once)
        };
        if self.take_focus && !focused_once {
            state.update(cx, |s, _| s.focused_once = true);
            let f = focus.clone();
            window.defer(cx, move |window, cx| window.focus(&f, cx));
        }
        let pickable: Rc<Vec<bool>> = Rc::new(
            self.entries
                .iter()
                .map(|e| matches!(e, Entry::Item(i) if !i.disabled))
                .collect(),
        );
        let ids: Rc<Vec<Option<SharedString>>> = Rc::new(
            self.entries
                .iter()
                .map(|e| match e {
                    Entry::Item(i) => Some(i.id.clone()),
                    Entry::Separator | Entry::Heading(_) => None,
                })
                .collect(),
        );
        let is_sub: Rc<Vec<bool>> = Rc::new(
            self.entries
                .iter()
                .map(|e| matches!(e, Entry::Item(i) if i.submenu.is_some()))
                .collect(),
        );
        // An entry's index by its id, if it has a submenu.
        let sub_at = {
            let (ids, is_sub) = (ids.clone(), is_sub.clone());
            move |id: &SharedString| {
                ids.iter()
                    .position(|i| i.as_ref() == Some(id))
                    .filter(|i| is_sub[*i])
            }
        };
        // Where it opens: the host's entry, else the first that can be picked; with the host's
        // submenu open, on its entry.
        let open_submenu = self.open_submenu.clone();
        // Whether the keyboard is in an open submenu (inside this menu, but not on it).
        let in_submenu = focus.contains_focused(window, cx) && !focus.is_focused(window);
        let mut gone = false;
        let highlighted = state.update(cx, |s, _| {
            // Where the highlighted entry is now: by its id; if it's gone, the first that can be
            // picked.
            if s.opened && s.highlighted_id.is_some() {
                s.highlighted = s
                    .highlighted_id
                    .as_ref()
                    .and_then(|want| ids.iter().position(|id| id.as_ref() == Some(want)))
                    .filter(|i| pickable[*i])
                    // Gone: the entry now at its place, or the nearest that can be picked.
                    .or_else(|| nearest(&pickable, s.highlighted))
                    .or_else(|| step(&pickable, None, 1));
            }
            if !s.opened
                && let Some(id) = &open_submenu
                && let Some(i) = sub_at(id)
            {
                s.opened = true;
                s.open_sub = Some(id.clone());
                s.sub_by_pointer = false;
                s.highlighted = Some(i);
            }
            // An open submenu whose entry has gone, or lost its submenu, is closed.
            if s.open_sub.as_ref().is_some_and(|id| sub_at(id).is_none()) {
                s.open_sub = None;
                gone = true;
            }
            if !s.opened {
                s.opened = true;
                let given = self.highlight.as_ref().and_then(|want| {
                    ids.iter()
                        .position(|id| id.as_ref() == Some(want))
                        .filter(|i| pickable[*i])
                });
                s.highlighted = given.or_else(|| {
                    if self.start_blank {
                        None
                    } else {
                        step(&pickable, None, 1)
                    }
                });
            }
            s.highlighted_id = s.highlighted.and_then(|i| ids.get(i).cloned().flatten());
            s.highlighted
        });
        // A submenu that had the keyboard went with its entry: the keyboard comes back here, so
        // the keys (Escape too) still work.
        if gone && in_submenu {
            let f = focus.clone();
            window.defer(cx, move |window, cx| window.focus(&f, cx));
        }
        let on_select = self.on_select.clone();
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
                        s.highlighted_id = to.and_then(|i| ids.get(i).cloned().flatten());
                        cx.notify();
                    }
                    moved
                });
                if moved && let Some(f) = &on_highlight {
                    f(to.and_then(|i| ids.get(i)).and_then(|id| id.as_ref()), cx);
                }
            }
        });

        // Closes the open submenu, giving the keyboard back to this menu.
        let close: CloseFn = Rc::new({
            let state = state.clone();
            let focus = focus.clone();
            move |window: &mut Window, cx: &mut App| {
                let closed = state.update(cx, |s, cx| {
                    s.rest += 1;
                    let closed = s.open_sub.take().is_some();
                    if closed {
                        cx.notify();
                    }
                    closed
                });
                if closed {
                    window.focus(&focus, cx);
                }
            }
        });
        // Opens entry `i`'s submenu (it takes the keyboard when it's first drawn).
        let open: OpenFn = Rc::new({
            let state = state.clone();
            let is_sub = is_sub.clone();
            let ids = ids.clone();
            let set = set.clone();
            move |i, by_pointer, _: &mut Window, cx: &mut App| {
                if is_sub.get(i) != Some(&true) {
                    return;
                }
                set(Some(i), cx);
                let id = ids[i].clone();
                state.update(cx, |s, cx| {
                    s.rest += 1;
                    if s.open_sub != id {
                        s.open_sub = id;
                        s.sub_by_pointer = by_pointer;
                        cx.notify();
                    }
                });
            }
        });
        let subs = is_sub.iter().any(|s| *s).then(|| {
            let (open_sub, by_pointer) = {
                let s = state.read(cx);
                (s.open_sub.as_ref().and_then(&sub_at), s.sub_by_pointer)
            };
            // The pointer resting on entry `i`: after a moment, its submenu opens, or the open
            // one closes if it has none.
            let rest: RestFn = Rc::new({
                let state = state.clone();
                let is_sub = is_sub.clone();
                let sub_at = sub_at.clone();
                let (open, close) = (open.clone(), close.clone());
                move |i, window: &mut Window, cx: &mut App| {
                    let (now, open_sub) = state.update(cx, |s, _| {
                        s.rest += 1;
                        (s.rest, s.open_sub.as_ref().and_then(&sub_at))
                    });
                    let wanted = (is_sub.get(i) == Some(&true)).then_some(i);
                    if wanted == open_sub {
                        return;
                    }
                    let (state, open, close) = (state.clone(), open.clone(), close.clone());
                    window
                        .spawn(cx, async move |cx| {
                            cx.background_executor().timer(SUBMENU_DELAY).await;
                            cx.update(|window, cx| {
                                if state.read(cx).rest != now {
                                    return;
                                }
                                match wanted {
                                    Some(i) => open(i, true, window, cx),
                                    None => close(window, cx),
                                }
                            })
                            .ok();
                        })
                        .detach();
                }
            });
            // The pointer reaching the open submenu: what it was opened from stays highlighted,
            // and a rest on the way there is forgotten.
            let entered: Rc<dyn Fn(&mut App)> = Rc::new({
                let state = state.clone();
                let sub_at = sub_at.clone();
                move |cx: &mut App| {
                    state.update(cx, |s, cx| {
                        s.rest += 1;
                        let at = s.open_sub.as_ref().and_then(&sub_at);
                        if at.is_some() && s.highlighted != at {
                            s.highlighted = at;
                            s.highlighted_id = s.open_sub.clone();
                            cx.notify();
                        }
                    })
                }
            });
            Subs {
                open_sub,
                by_pointer,
                open: open.clone(),
                rest,
                entered,
                close: close.clone(),
            }
        });

        let mover = |delta: isize| {
            let state = state.clone();
            let pickable = pickable.clone();
            let set = set.clone();
            let close = close.clone();
            move |window: &mut Window, cx: &mut App| {
                let from = state.read(cx).highlighted;
                let to = match delta {
                    isize::MIN => step(&pickable, None, 1),
                    isize::MAX => step(&pickable, None, -1),
                    d => step(&pickable, from, d),
                };
                set(to, cx);
                close(window, cx);
            }
        };
        let (prev, next, first, last) = (mover(-1), mover(1), mover(isize::MIN), mover(isize::MAX));
        let choose = Rc::new({
            let state = state.clone();
            let ids = ids.clone();
            let pickable = pickable.clone();
            let on_select = on_select.clone();
            let is_sub = is_sub.clone();
            let open = open.clone();
            move |modifiers: Modifiers, window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let Some(i) = at
                    && is_sub.get(i) == Some(&true)
                {
                    open(i, false, window, cx);
                    return;
                }
                if let (Some(i), Some(f)) = (at, &on_select)
                    && pickable.get(i) == Some(&true)
                    && let Some(Some(id)) = ids.get(i)
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
        let open_highlighted = {
            let state = state.clone();
            let is_sub = is_sub.clone();
            move |window: &mut Window, cx: &mut App| match state.read(cx).highlighted {
                Some(i) if is_sub.get(i) == Some(&true) => open(i, false, window, cx),
                _ => cx.propagate(),
            }
        };

        let rows = rows_with(
            &self.id,
            self.entries,
            0,
            highlighted,
            self.row,
            self.colors,
            Rc::new(move |i, cx: &mut App| set(Some(i), cx)),
            on_select,
            subs,
            cx,
        );

        let nested = self.nested;
        div()
            .id(self.id)
            .role(Role::Menu)
            .key_context(if nested.is_some() {
                SUBMENU_CONTEXT
            } else {
                CONTEXT
            })
            .track_focus(&focus)
            .flex()
            .flex_col()
            .min_w_0()
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &First, w, cx| first(w, cx))
            .on_action(move |_: &Last, w, cx| last(w, cx))
            .on_action(move |_: &Choose, w, cx| choose(Modifiers::none(), w, cx))
            .on_action(move |_: &ChooseShift, w, cx| choose_shift(shift(), w, cx))
            .on_action(move |_: &ChooseSecondary, w, cx| choose_secondary(secondary(cx), w, cx))
            .on_action(move |_: &OpenSubmenu, w, cx| open_highlighted(w, cx))
            .when_some(nested, |d, close| {
                d.on_action(move |_: &CloseSubmenu, w, cx| close(w, cx))
            })
            .children(rows)
    }
}

/// Opens an entry's submenu; `true` when the pointer opened it.
type OpenFn = Rc<dyn Fn(usize, bool, &mut Window, &mut App)>;
/// The pointer resting on an entry.
type RestFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A menu's submenus, for drawing its rows: which is open, and what opens and closes them.
pub(crate) struct Subs {
    open_sub: Option<usize>,
    by_pointer: bool,
    open: OpenFn,
    rest: RestFn,
    entered: Rc<dyn Fn(&mut App)>,
    close: CloseFn,
}

/// A menu's entries drawn as rows: `highlighted` filled, the pointer highlighting what it's
/// over through `set_highlight`, a press picking through `on_select`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rows(
    menu_id: &ElementId,
    entries: Vec<Entry>,
    // The first entry's index in the whole menu (a virtual list draws a slice).
    first: usize,
    highlighted: Option<usize>,
    row_style: RowStyle,
    colors: PopupMenuColors,
    set_highlight: SetHighlight,
    on_select: Option<OnSelect>,
    cx: &App,
) -> Vec<AnyElement> {
    rows_with(
        menu_id,
        entries,
        first,
        highlighted,
        row_style,
        colors,
        set_highlight,
        on_select,
        None,
        cx,
    )
}

/// [`rows`], with what opens and closes the menu's submenus.
#[allow(clippy::too_many_arguments)]
fn rows_with(
    menu_id: &ElementId,
    entries: Vec<Entry>,
    first: usize,
    highlighted: Option<usize>,
    row_style: RowStyle,
    colors: PopupMenuColors,
    set_highlight: SetHighlight,
    on_select: Option<OnSelect>,
    subs: Option<Subs>,
    cx: &App,
) -> Vec<AnyElement> {
    let theme: Theme = cx.theme().clone();
    let highlight = colors.highlight.unwrap_or(theme.accent);
    let highlight_text = colors.highlight_text.unwrap_or(theme.accent_foreground);
    let muted = colors.muted.unwrap_or(theme.muted_foreground);
    let separator = colors.separator.unwrap_or(theme.border);
    let row_h: gpui::DefiniteLength = row_style.height.map(Into::into).unwrap_or(css(32.).into());
    let touch_h: Option<gpui::Pixels> = row_style.touch.then_some(theme.touch_target);
    let text_size: gpui::AbsoluteLength = row_style.text_size.unwrap_or(theme.text.sm.into());
    let menu_id = std::sync::Arc::new(menu_id.clone());
    let on_select_for_subs = on_select.clone();
    // A submenu's panel is drawn in the colours of the panel this menu is in.
    let panel_colors = crate::popover::panel_colors(cx);
    entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| (first + i, e))
        .map(|(i, e)| {
            let item = match e {
                Entry::Separator => {
                    return div()
                        .flex_none()
                        .h(gpui::px(1.))
                        .my(css(4.))
                        .mx(css(-4.))
                        .bg(separator)
                        .into_any_element();
                }
                Entry::Heading(title) => {
                    return div()
                        .flex_none()
                        .px(css(8.))
                        .pt(css(6.))
                        .pb(css(2.))
                        .text_xs()
                        .when_some(row_style.heading_size, |d, size| d.text_size(size))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(colors.heading.unwrap_or(muted))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(title)
                        .into_any_element();
                }
                Entry::Item(item) => item,
            };
            let mut item = item;
            let on = highlighted == Some(i);
            let submenu = item.submenu.take();
            let opens = item.opens;
            let open_now = subs.as_ref().is_some_and(|s| s.open_sub == Some(i));
            let id = item.id.clone();
            let role = match item.checked {
                Some(_) => Role::MenuItemCheckBox,
                None => Role::MenuItem,
            };
            let set_highlight = set_highlight.clone();
            // A disabled entry can't be highlighted, so Enter can't pick it.
            let disabled = item.disabled;
            let on_select = on_select.clone();
            let rest = subs.as_ref().map(|s| s.rest.clone());
            let open = subs
                .as_ref()
                .filter(|_| submenu.is_some())
                .map(|s| s.open.clone());
            let mut row = Press::new(ElementId::NamedChild(menu_id.clone(), id.clone()))
                .role(role)
                .tab_stop(false)
                .focus_on_press(false)
                .focus_ring(false)
                .disabled(item.disabled)
                .accessibility_label(item.label.clone())
                .debug_selector({
                    let id = id.clone();
                    move || format!("menu-item-{id}")
                })
                .flex()
                .items_center()
                .gap(css(8.))
                .w_full()
                .min_w_0()
                .min_h(row_h)
                .when_some(touch_h, |r, h| r.min_h(h))
                .px(css(8.))
                .rounded(theme.radius_sm())
                .text_size(text_size)
                .when(on, |r| r.bg(highlight).text_color(highlight_text))
                .when(item.disabled, |r| r.text_color(muted).opacity(0.6))
                .when(opens, |r| r.aria_expanded(open_now))
                .on_hover(move |hovered, window, cx| {
                    if *hovered && !disabled {
                        set_highlight(i, cx);
                        if let Some(rest) = &rest {
                            rest(i, window, cx);
                        }
                    }
                })
                .on_click(move |e: &ClickEvent, window, cx| {
                    if let Some(open) = &open {
                        open(i, true, window, cx);
                        return;
                    }
                    if let Some(f) = &on_select {
                        let pick = MenuSelect {
                            id: id.clone(),
                            modifiers: e.modifiers(),
                            query: None,
                        };
                        f(&pick, window, cx);
                    }
                });
            if let Some(checked) = item.checked {
                row = row.selected(checked);
            }
            let id_for_check = item.id.clone();
            let id_for_panel = item.id.clone();
            let on_select_for_sub = on_select_for_subs.clone();
            let check = || Icon::new(IconName::Check).when_some(colors.check, |i, c| i.color(c));
            let checked = item.checked == Some(true);
            let row = row
                .when(row_style.check_leading, |r| {
                    // The column: the check, or the room for it.
                    r.child(
                        div()
                            .flex_none()
                            .w(css(16.))
                            .flex()
                            .justify_center()
                            .debug_selector({
                                let id = id_for_check.clone();
                                move || format!("menu-check-{id}")
                            })
                            .when(checked, |d| d.child(check())),
                    )
                })
                .when_some(item.chip, |r, chip| r.child(chip))
                .when_some(item.icon, |r, icon| {
                    r.child(Icon::new(icon).color(if on { highlight_text } else { muted }))
                })
                .child(
                    // As wide as its text, filling the row: a long hint gives way first.
                    div()
                        .flex_auto()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .debug_selector({
                            let id = id_for_panel.clone();
                            move || format!("menu-label-{id}")
                        })
                        // Room either side for ink past a glyph's advance.
                        .px(css(3.))
                        .mx(css(-3.))
                        .map(|d| match item.label_element {
                            Some(el) => d.flex().items_center().child(el),
                            None => d.child(item.label),
                        }),
                )
                .when_some(item.shortcut, |r, s| {
                    let id = id_for_panel.clone();
                    let mut hint = div()
                        .min_w(css(HINT_MIN))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .debug_selector(move || format!("menu-hint-{id}"));
                    // It shrinks long before the label does: the label keeps its text until the
                    // hint is down to its least, then both are cut short.
                    hint.style().flex_shrink = Some(HINT_SHRINK);
                    r.child(
                        hint.text_xs()
                            .text_color(if on { highlight_text } else { muted })
                            .child(s),
                    )
                })
                .when(checked && !row_style.check_leading, |r| r.child(check()))
                .when(opens, |r| {
                    r.child(Icon::new(IconName::ChevronRight).color(if on {
                        highlight_text
                    } else {
                        muted
                    }))
                });
            let panel = submenu
                .filter(|_| open_now)
                .zip(subs.as_ref())
                .map(|(menu, subs)| {
                    let mut menu = menu.make();
                    menu.nested = Some(subs.close.clone());
                    menu.start_blank = subs.by_pointer;
                    menu.row = row_style;
                    menu.colors = colors;
                    if menu.on_select.is_none() {
                        menu.on_select = on_select_for_sub.clone();
                    }
                    let entered = subs.entered.clone();
                    let id = id_for_panel.clone();
                    let key = ElementId::NamedChild(
                        menu_id.clone(),
                        format!("{}-submenu", id_for_panel).into(),
                    );
                    (
                        key.clone(),
                        div()
                            .id(key)
                            .debug_selector(move || format!("menu-submenu-{id}"))
                            .occlude()
                            .flex()
                            .flex_col()
                            .min_w(css(160.))
                            .overflow_y_scroll()
                            .p(css(4.))
                            .rounded(theme.radius_md())
                            .border_1()
                            .border_color(panel_colors.border)
                            .bg(panel_colors.background)
                            .text_color(panel_colors.foreground)
                            .shadow_md()
                            .on_hover(move |hovered, _, cx| {
                                if *hovered {
                                    entered(cx);
                                }
                            })
                            .child(menu),
                    )
                });
            match panel {
                Some((key, panel)) => SubmenuElement {
                    key,
                    colors: panel_colors,
                    row: row.into_any_element(),
                    panel: Some(panel),
                }
                .into_any_element(),
                None => row.into_any_element(),
            }
        })
        .collect()
}

/// The least a cut-short hint keeps (room for a character or two and the ellipsis), in CSS
/// pixels, and how much faster than the label it shrinks.
const HINT_MIN: f32 = 28.;
const HINT_SHRINK: f32 = 1000.;

/// How far a submenu keeps from the window's edges, and how far it sits out from its entry (the
/// menu's padding and border), in CSS pixels.
const EDGE: f32 = 8.;
const INSET: f32 = 5.;

/// Where a submenu of `size` goes beside its entry `row`: to the right with its first entry
/// level with `row`, or to the left where the right has no room; moved in to keep `edge` from
/// the window's sides.
pub(crate) fn place_beside(
    row: Bounds<Pixels>,
    size: gpui::Size<Pixels>,
    view: Bounds<Pixels>,
    edge: Pixels,
    inset: Pixels,
) -> Point<Pixels> {
    let right = row.right() + inset;
    let x = if right + size.width <= view.right() - edge {
        right
    } else {
        let left = row.left() - inset - size.width;
        // Where neither side has room, the side with more.
        if left >= view.left() + edge || row.left() - view.left() > view.right() - row.right() {
            left
        } else {
            right
        }
    };
    let x = x
        .min(view.right() - edge - size.width)
        .max(view.left() + edge);
    let y = (row.top() - inset)
        .min(view.bottom() - edge - size.height)
        .max(view.top() + edge);
    point(x.round(), y.round())
}

/// An entry drawn in place, with its open submenu over everything beside it.
struct SubmenuElement {
    /// The panel's id, which its claim on presses is kept under.
    key: ElementId,
    /// The panel's colours, which a submenu inside it takes.
    colors: crate::PopoverColors,
    row: AnyElement,
    panel: Option<Stateful<gpui::Div>>,
}

impl IntoElement for SubmenuElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SubmenuElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.row.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.row.prepaint(window, cx);
        let Some(panel) = self.panel.take() else {
            return;
        };
        let view = Bounds::new(Point::default(), window.viewport_size());
        let rem = window.rem_size();
        let (edge, inset) = (rem * (EDGE / 15.), rem * (INSET / 15.));
        let panel = panel
            .max_h((view.size.height - edge * 2.).max(px(0.)))
            .max_w((view.size.width - edge * 2.).max(px(0.)))
            .into_any_element();
        // A press on it isn't one outside the popover the menu is in.
        let mut panel = ClaimArea::new(self.key.clone(), panel).into_any_element();
        // Laid out on its own, now that its entry has been placed this frame.
        let size = crate::popover::with_panel_colors(self.colors, window, cx, |window, cx| {
            panel.layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::MaxContent,
                    gpui::AvailableSpace::MaxContent,
                ),
                window,
                cx,
            )
        });
        let origin = place_beside(bounds, size, view, edge, inset);
        window.defer_draw(
            panel,
            origin,
            crate::popover::PRIORITY,
            None::<ContentMask<Pixels>>,
        );
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.row.paint(window, cx);
    }
}
