//! A menubar: a row of menus (File, Edit, View) in the window, as shadcn's Menubar. Each opens a
//! [`PopupMenu`] below it. The row has one Tab stop; Left and Right move along it, and Down,
//! Enter or Space opens a menu. While one is open, Left and Right open the menu beside it
//! (Right opens a submenu first where the highlighted entry has one), the pointer moving onto
//! another menu's title opens that one, and Escape or a press outside closes it.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{MenuSelect, Menubar, MenubarMenu, PopupMenu, PopupMenuItem};
//! # struct Editor;
//! # impl Render for Editor {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Menubar::new("menubar")
//!         .menu(MenubarMenu::new(
//!             "file",
//!             "File",
//!             PopupMenu::new("file-menu")
//!                 .item(PopupMenuItem::new("new", "New Tab").shortcut("⌘T"))
//!                 .item(PopupMenuItem::new("print", "Print…")),
//!         ))
//!         .menu(MenubarMenu::new(
//!             "edit",
//!             "Edit",
//!             PopupMenu::new("edit-menu").item(PopupMenuItem::new("undo", "Undo")),
//!         ))
//!         .on_select(cx.listener(|_, (menu, pick): &(SharedString, MenuSelect), _, _| {
//!             // `menu` is "file" or "edit", `pick.id` the entry's.
//!             let _ = (menu, pick);
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::menu::{MenuSelect, PopupMenu};
use crate::popover::Popover;
use crate::press::Press;
use crate::size::Disableable as _;
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_menubar,
    [
        /// The menu before: the keyboard moves to its title, or it opens if one's open.
        PreviousMenu,
        /// The menu after.
        NextMenu,
        /// Opens the menu whose title has the keyboard.
        OpenMenu
    ]
);

const CONTEXT: &str = "GpuiCoreMenubar";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", PreviousMenu, c),
        KeyBinding::new("right", NextMenu, c),
        KeyBinding::new("down", OpenMenu, c),
    ]);
}

/// What [`Menubar::on_select`] is told: the menu's id and the pick in it.
pub type MenubarSelect = (SharedString, MenuSelect);

type OnSelect = Rc<dyn Fn(&MenubarSelect, &mut Window, &mut App)>;

/// Opens a menu by its index, or closes the open one.
type SetOpen = Rc<dyn Fn(Option<usize>, &mut App)>;

/// One menu: its title in the bar and the [`PopupMenu`] it opens. The menu's picks go to
/// [`Menubar::on_select`] (its own `on_select` is replaced).
pub struct MenubarMenu {
    id: SharedString,
    label: SharedString,
    menu: PopupMenu,
    disabled: bool,
}

impl MenubarMenu {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        menu: PopupMenu,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            menu,
            disabled: false,
        }
    }

    /// Shown dimmed and passed over: it doesn't open.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A row of menus. It owns which is open; a pick closes it and goes to
/// [`on_select`](Self::on_select).
#[derive(IntoElement)]
pub struct Menubar {
    id: ElementId,
    menus: Vec<MenubarMenu>,
    on_select: Option<OnSelect>,
    label: Option<SharedString>,
    touch: bool,
    style: StyleRefinement,
}

impl Menubar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            menus: Vec::new(),
            on_select: None,
            label: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn menu(mut self, menu: MenubarMenu) -> Self {
        self.menus.push(menu);
        self
    }

    /// Told the menu and the entry picked in it.
    pub fn on_select(
        mut self,
        f: impl Fn(&MenubarSelect, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// What it is to assistive tech ("Application", say).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Titles and entries 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for Menubar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// The bar: a menu bar to assistive tech.
fn bar(id: &ElementId, label: Option<SharedString>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.clone())
        .role(Role::MenuBar)
        .when_some(label, |d, l| d.aria_label(l))
        .debug_selector(|| "menubar".into())
}

/// A menu's title in the bar: a menu item to assistive tech, expanded while its menu is open.
fn title(
    bar: &ElementId,
    m: &MenubarMenu,
    is_open: bool,
    focus: &FocusHandle,
    tab_stop: bool,
    touch: bool,
    theme: &Theme,
) -> Press {
    let selector = format!("menubar-{}", m.id);
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(bar.clone()),
        m.id.clone(),
    ))
    .role(Role::MenuItem)
    .aria_expanded(is_open)
    .accessibility_label(m.label.clone())
    .track_focus(focus)
    .tab_stop(tab_stop)
    .disabled(m.disabled)
    .debug_selector(move || selector.clone())
    .flex()
    .flex_none()
    .items_center()
    .px(css(8.))
    .py(css(4.))
    .when(touch, |p| p.min_h(theme.touch_target))
    .rounded(theme.radius_sm())
    .text_size(theme.text.sm)
    .font_weight(gpui::FontWeight::MEDIUM)
    .whitespace_nowrap()
    .when(is_open, |p| {
        p.bg(theme.accent).text_color(theme.accent_foreground)
    })
    .when(!is_open && !m.disabled, |p| {
        let (bg, fg) = (theme.accent, theme.accent_foreground);
        p.hover(move |s| s.bg(bg).text_color(fg))
    })
    .when(m.disabled, |p| p.opacity(0.5))
    .child(m.label.clone())
}

/// What a menubar remembers between frames.
#[derive(Default)]
struct MenubarState {
    focus: Vec<FocusHandle>,
    /// The open menu, by its id.
    open: Option<SharedString>,
    /// The title that's the bar's Tab stop: the last the keyboard or a menu was on.
    stop: Option<SharedString>,
}

/// The next of `enabled` from `from`, `step` at a time, wrapping.
fn step(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
    let n = enabled.len() as isize;
    let mut i = from as isize;
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        if enabled[i as usize] {
            return Some(i as usize);
        }
    }
    None
}

impl RenderOnce for Menubar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let theme = cx.theme().clone();
        let n = self.menus.len();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| MenubarState::default());
        state.update(cx, |s, cx| {
            while s.focus.len() < n {
                s.focus.push(cx.focus_handle());
            }
            s.focus.truncate(n);
        });
        let ids: Rc<Vec<SharedString>> = Rc::new(self.menus.iter().map(|m| m.id.clone()).collect());
        let enabled: Rc<Vec<bool>> = Rc::new(self.menus.iter().map(|m| !m.disabled).collect());
        let (handles, open, stop) = {
            let s = state.read(cx);
            let at = |id: &Option<SharedString>| {
                id.as_ref()
                    .and_then(|id| ids.iter().position(|i| i == id))
                    .filter(|i| enabled[*i])
            };
            (Rc::new(s.focus.clone()), at(&s.open), at(&s.stop))
        };
        let stop = stop.or_else(|| enabled.iter().position(|e| *e));

        // Opens menu `to` (or closes the open one), keeping its title the Tab stop.
        let set_open: SetOpen = Rc::new({
            let (state, ids) = (state.clone(), ids.clone());
            move |to, cx| {
                state.update(cx, |s, cx| {
                    let id = to.map(|i| ids[i].clone());
                    if s.open != id {
                        s.open = id.clone();
                        if id.is_some() {
                            s.stop = id;
                        }
                        cx.notify();
                    }
                })
            }
        });
        // Left or Right: the menu beside the open one opens; with none open, the keyboard moves
        // to the title beside.
        let mover = |delta: isize| {
            let (handles, enabled, set_open, state, ids) = (
                handles.clone(),
                enabled.clone(),
                set_open.clone(),
                state.clone(),
                ids.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                let from = open
                    .or_else(|| handles.iter().position(|h| h.is_focused(window)))
                    .or(stop)
                    .unwrap_or(0);
                let Some(to) = step(&enabled, from, delta) else {
                    return;
                };
                if open.is_some() {
                    set_open(Some(to), cx);
                } else {
                    state.update(cx, |s, cx| {
                        s.stop = Some(ids[to].clone());
                        cx.notify();
                    });
                    window.focus(&handles[to], cx);
                }
            }
        };
        let (prev, next) = (mover(-1), mover(1));
        let open_focused = {
            let (handles, enabled, set_open) = (handles.clone(), enabled.clone(), set_open.clone());
            move |window: &mut Window, cx: &mut App| {
                if open.is_some() {
                    cx.propagate();
                    return;
                }
                if let Some(i) = handles.iter().position(|h| h.is_focused(window))
                    && enabled[i]
                {
                    set_open(Some(i), cx);
                }
            }
        };

        let touch = self.touch;
        let menus = self.menus.into_iter().enumerate().map(|(i, m)| {
            let is_open = open == Some(i);
            let toggle = set_open.clone();
            let hover = set_open.clone();
            let dismiss = set_open.clone();
            let picked = set_open.clone();
            let on_select = self.on_select.clone();
            let menu_id = m.id.clone();
            let trigger = title(
                &self.id,
                &m,
                is_open,
                &handles[i],
                stop == Some(i),
                touch,
                &theme,
            )
            .on_hover(move |hovered, _, cx| {
                // While a menu is open, the pointer on another's title opens that one.
                if *hovered && open.is_some() && open != Some(i) {
                    hover(Some(i), cx);
                }
            })
            .on_click(move |_, _, cx| toggle(if is_open { None } else { Some(i) }, cx));
            let menu = m
                .menu
                .touch(touch)
                .on_select(move |pick: &MenuSelect, window, cx| {
                    picked(None, cx);
                    if let Some(f) = &on_select {
                        f(&(menu_id.clone(), pick.clone()), window, cx);
                    }
                });
            Popover::new(ElementId::NamedChild(
                std::sync::Arc::new(self.id.clone()),
                format!("{}-popover", m.id).into(),
            ))
            .open(is_open)
            .role(Role::Menu)
            .accessibility_label(m.label)
            .return_focus(&handles[i])
            .match_trigger_width(false)
            .on_dismiss(move |_, _, cx| dismiss(None, cx))
            .trigger(trigger)
            .min_w(css(192.))
            .p(css(4.))
            .child(menu)
        });

        let mut bar = bar(&self.id, self.label)
            .key_context(CONTEXT)
            .on_action(move |_: &PreviousMenu, w, cx| prev(w, cx))
            .on_action(move |_: &NextMenu, w, cx| next(w, cx))
            .on_action(move |_: &OpenMenu, w, cx| open_focused(w, cx))
            .flex()
            .items_center()
            .gap(css(4.))
            .min_w_0()
            .max_w_full()
            .overflow_x_hidden()
            .when(!touch, |d| d.h(css(36.)))
            .p(css(4.))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_xs()
            .children(menus);
        gpui::Refineable::refine(bar.style(), &self.style);
        bar
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    #[gpui::test]
    fn the_bar_and_its_titles_have_their_roles(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let bar = bar(&"m".into(), Some("Application".into()));
            let mut node = accesskit::Node::new(Role::Unknown);
            bar.write_a11y_info(&mut node);
            assert_eq!(bar.a11y_role(), Some(Role::MenuBar));
            assert_eq!(node.label(), Some("Application"));

            let theme = cx.theme().clone();
            let focus = cx.focus_handle();
            let m = MenubarMenu::new("file", "File", PopupMenu::new("f"));
            for open in [false, true] {
                let t = title(&"m".into(), &m, open, &focus, true, false, &theme)
                    .render(window, cx)
                    .into_element();
                let mut node = accesskit::Node::new(Role::Unknown);
                t.write_a11y_info(&mut node);
                assert_eq!(t.a11y_role(), Some(Role::MenuItem));
                assert_eq!(node.label(), Some("File"));
                assert_eq!(node.is_expanded(), Some(open));
            }
        });
    }
}
