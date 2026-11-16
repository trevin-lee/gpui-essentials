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
//!                 .on_select(move |pick, window, cx| {
//!                     // `pick.id` is the entry's: "copy" or "wrap"; `pick.modifiers` what's held.
//!                     close(&(), window, cx)
//!                 }),
//!         )
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, Hsla, InteractiveElement as _,
    IntoElement, KeyBinding, Modifiers, ParentElement as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
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
        ChooseSecondary
    ]
);

const CONTEXT: &str = "GpuiCoreMenu";

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
    ]);
}

/// A pick: the entry's id, and the modifiers held (Shift-Enter or a Shift-click to add rather
/// than replace, say). From the keyboard, Shift-Enter and Cmd-Enter (Ctrl-Enter off the Mac)
/// come with those.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSelect {
    pub id: SharedString,
    pub modifiers: Modifiers,
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
    on_select: Option<OnSelect>,
    on_highlight: Option<OnHighlight>,
    highlight: Option<SharedString>,
    row: RowStyle,
    take_focus: bool,
    colors: PopupMenuColors,
}

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
        }
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
    highlighted: Option<usize>,
    focused_once: bool,
    /// Whether it has set its first highlight.
    opened: bool,
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
            focused_once: false,
            opened: false,
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
        // Where it opens: the host's entry, else the first that can be picked.
        let highlighted = state.update(cx, |s, _| {
            if !s.opened {
                s.opened = true;
                let given = self.highlight.as_ref().and_then(|want| {
                    ids.iter()
                        .position(|id| id.as_ref() == Some(want))
                        .filter(|i| pickable[*i])
                });
                s.highlighted = given.or_else(|| step(&pickable, None, 1));
            }
            s.highlighted
        });
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
                        cx.notify();
                    }
                    moved
                });
                if moved && let Some(f) = &on_highlight {
                    f(to.and_then(|i| ids.get(i)).and_then(|id| id.as_ref()), cx);
                }
            }
        });

        let mover = |delta: isize| {
            let state = state.clone();
            let pickable = pickable.clone();
            let set = set.clone();
            move |_: &mut Window, cx: &mut App| {
                let from = state.read(cx).highlighted;
                let to = match delta {
                    isize::MIN => step(&pickable, None, 1),
                    isize::MAX => step(&pickable, None, -1),
                    d => step(&pickable, from, d),
                };
                set(to, cx)
            }
        };
        let (prev, next, first, last) = (mover(-1), mover(1), mover(isize::MIN), mover(isize::MAX));
        let choose = Rc::new({
            let state = state.clone();
            let ids = ids.clone();
            let pickable = pickable.clone();
            let on_select = on_select.clone();
            move |modifiers: Modifiers, window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let (Some(i), Some(f)) = (at, &on_select)
                    && pickable.get(i) == Some(&true)
                    && let Some(Some(id)) = ids.get(i)
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

        let rows = rows(
            &self.id,
            self.entries,
            0,
            highlighted,
            self.row,
            self.colors,
            Rc::new(move |i, cx: &mut App| set(Some(i), cx)),
            on_select,
            cx,
        );

        div()
            .id(self.id)
            .role(Role::Menu)
            .key_context(CONTEXT)
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
            .children(rows)
    }
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
    let theme: Theme = cx.theme().clone();
    let highlight = colors.highlight.unwrap_or(theme.accent);
    let highlight_text = colors.highlight_text.unwrap_or(theme.accent_foreground);
    let muted = colors.muted.unwrap_or(theme.muted_foreground);
    let separator = colors.separator.unwrap_or(theme.border);
    let row_h: gpui::DefiniteLength = row_style.height.map(Into::into).unwrap_or(css(32.).into());
    let touch_h: Option<gpui::Pixels> = row_style.touch.then_some(theme.touch_target);
    let text_size: gpui::AbsoluteLength = row_style.text_size.unwrap_or(theme.text.sm.into());
    let menu_id = std::sync::Arc::new(menu_id.clone());
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
            let on = highlighted == Some(i);
            let id = item.id.clone();
            let role = match item.checked {
                Some(_) => Role::MenuItemCheckBox,
                None => Role::MenuItem,
            };
            let set_highlight = set_highlight.clone();
            // A disabled entry can't be highlighted, so Enter can't pick it.
            let disabled = item.disabled;
            let on_select = on_select.clone();
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
                .on_hover(move |hovered, _, cx| {
                    if *hovered && !disabled {
                        set_highlight(i, cx);
                    }
                })
                .on_click(move |e: &ClickEvent, window, cx| {
                    if let Some(f) = &on_select {
                        let pick = MenuSelect {
                            id: id.clone(),
                            modifiers: e.modifiers(),
                        };
                        f(&pick, window, cx);
                    }
                });
            if let Some(checked) = item.checked {
                row = row.selected(checked);
            }
            row.when_some(item.chip, |r, chip| r.child(chip))
                .when_some(item.icon, |r, icon| {
                    r.child(Icon::new(icon).color(if on { highlight_text } else { muted }))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        // Room either side for ink past a glyph's advance.
                        .px(css(3.))
                        .mx(css(-3.))
                        .map(|d| match item.label_element {
                            Some(el) => d.flex().items_center().child(el),
                            None => d.child(item.label),
                        }),
                )
                .when_some(item.shortcut, |r, s| {
                    r.child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(if on { highlight_text } else { muted })
                            .child(s),
                    )
                })
                .when(item.checked == Some(true), |r| {
                    r.child(Icon::new(IconName::Check).when_some(colors.check, |i, c| i.color(c)))
                })
                .into_any_element()
        })
        .collect()
}
