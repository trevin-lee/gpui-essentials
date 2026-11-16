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
//!                 .on_select(move |id, window, cx| {
//!                     // `id` is the entry's: "copy" or "wrap".
//!                     close(&(), window, cx)
//!                 }),
//!         )
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::press::Press;
use crate::size::{Disableable as _, Selectable as _};
use crate::theme::{ActiveTheme as _, Theme};

actions!(gpui_core_menu, [Previous, Next, First, Last, Choose]);

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
    ]);
}

/// One entry: a label, and optionally an icon, a check, a shortcut's text and a chip the host
/// draws (a select option's colour).
pub struct PopupMenuItem {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
    checked: Option<bool>,
    disabled: bool,
    shortcut: Option<SharedString>,
    chip: Option<AnyElement>,
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

    /// Something the host draws before the label: a colour's swatch, an avatar.
    pub fn chip(mut self, chip: impl IntoElement) -> Self {
        self.chip = Some(chip.into_any_element());
        self
    }
}

enum Entry {
    Item(PopupMenuItem),
    Separator,
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
}

type OnSelect = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// A list of [`PopupMenuItem`]s and separators. Up and Down move a highlight over the entries that
/// can be picked (the pointer moves it too), Home and End go to the ends, Enter or Space picks,
/// and a press picks. It takes the keyboard when it's first drawn, so in a
/// [`Popover`](crate::Popover) the keys work at once and Escape closes the popover.
#[derive(IntoElement)]
pub struct PopupMenu {
    id: ElementId,
    entries: Vec<Entry>,
    on_select: Option<OnSelect>,
    touch: bool,
    take_focus: bool,
    colors: PopupMenuColors,
}

impl PopupMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            entries: Vec::new(),
            on_select: None,
            touch: false,
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

    /// Runs with the picked entry's id. A host whose menu is in a popover closes it here.
    pub fn on_select(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Entries 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
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
}

/// The next entry from `from` in `step`'s direction that can be picked, wrapping.
fn step(pickable: &[bool], from: Option<usize>, step: isize) -> Option<usize> {
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
        });
        let (focus, highlighted, focused_once) = {
            let s = state.read(cx);
            (s.focus.clone(), s.highlighted, s.focused_once)
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
                    Entry::Separator => None,
                })
                .collect(),
        );
        let theme: Theme = cx.theme().clone();
        let colors = self.colors;
        let highlight = colors.highlight.unwrap_or(theme.accent);
        let highlight_text = colors.highlight_text.unwrap_or(theme.accent_foreground);
        let muted = colors.muted.unwrap_or(theme.muted_foreground);
        let separator = colors.separator.unwrap_or(theme.border);
        let row_h: gpui::DefiniteLength = if self.touch {
            theme.touch_target.into()
        } else {
            css(32.).into()
        };
        let on_select = self.on_select.clone();

        let mover = |delta: isize| {
            let state = state.clone();
            let pickable = pickable.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = window;
                state.update(cx, |s, cx| {
                    let to = match delta {
                        isize::MIN => step(&pickable, None, 1),
                        isize::MAX => step(&pickable, None, -1),
                        d => step(&pickable, s.highlighted, d),
                    };
                    if to != s.highlighted {
                        s.highlighted = to;
                        cx.notify();
                    }
                })
            }
        };
        let (prev, next, first, last) = (mover(-1), mover(1), mover(isize::MIN), mover(isize::MAX));
        let choose = {
            let state = state.clone();
            let ids = ids.clone();
            let pickable = pickable.clone();
            let on_select = on_select.clone();
            move |window: &mut Window, cx: &mut App| {
                let at = state.read(cx).highlighted;
                if let (Some(i), Some(f)) = (at, &on_select)
                    && pickable.get(i) == Some(&true)
                    && let Some(Some(id)) = ids.get(i)
                {
                    f(id, window, cx);
                }
            }
        };

        let menu_id = std::sync::Arc::new(self.id.clone());
        let rows: Vec<AnyElement> = self
            .entries
            .into_iter()
            .enumerate()
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
                    Entry::Item(item) => item,
                };
                let on = highlighted == Some(i);
                let id = item.id.clone();
                let role = match item.checked {
                    Some(_) => Role::MenuItemCheckBox,
                    None => Role::MenuItem,
                };
                let state_hover = state.clone();
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
                    .px(css(8.))
                    .rounded(theme.radius_sm())
                    .text_size(theme.text.sm)
                    .when(on, |r| r.bg(highlight).text_color(highlight_text))
                    .when(item.disabled, |r| r.text_color(muted).opacity(0.6))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered && !disabled {
                            state_hover.update(cx, |s, cx| {
                                if s.highlighted != Some(i) {
                                    s.highlighted = Some(i);
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .on_click(move |_, window, cx| {
                        if let Some(f) = &on_select {
                            f(&id, window, cx);
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
                            .child(item.label),
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
                        r.child(Icon::new(IconName::Check))
                    })
                    .into_any_element()
            })
            .collect();

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
            .on_action(move |_: &Choose, w, cx| choose(w, cx))
            .children(rows)
    }
}
