//! A navigation menu: a site's top navigation, as shadcn's NavigationMenu. Its items are links,
//! or triggers that open a panel of links below them, each with a title and a line or two of
//! description, in a grid.
//!
//! A trigger opens its panel on a press, Enter, Space or Down; opened from the keyboard, the
//! keyboard moves into the panel, where the arrows, Home and End move between its links. Left
//! and Right move between the items in the row; while a panel is open, the pointer moving onto
//! another trigger opens that one. Escape or a press outside closes it, the focus going back to
//! its trigger.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{NavigationMenu, NavigationMenuItem, NavigationMenuLink};
//! # struct Site;
//! # impl Render for Site {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     NavigationMenu::new("site")
//!         .item(
//!             NavigationMenuItem::panel("components", "Components")
//!                 .columns(2)
//!                 .with_link(
//!                     NavigationMenuLink::new("dialog", "Dialog")
//!                         .description("A window over the page that takes the keyboard."),
//!                 )
//!                 .with_link(
//!                     NavigationMenuLink::new("tabs", "Tabs")
//!                         .description("Panels shown one at a time."),
//!                 ),
//!         )
//!         .item(NavigationMenuItem::link("docs", "Docs"))
//!         .on_navigate(cx.listener(|_, id: &SharedString, _, _| {
//!             // Go to `id`.
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

use crate::icon::{Icon, IconName};
use crate::popover::Popover;
use crate::press::Press;
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_navigation_menu,
    [
        /// The item before in the row.
        PreviousItem,
        /// The item after in the row.
        NextItem,
        /// Opens the panel of the trigger with the keyboard.
        OpenItem,
        /// Closes the open panel.
        CloseItem,
        /// The link before in the open panel.
        PreviousLink,
        /// The link after in the open panel.
        NextLink,
        FirstLink,
        LastLink
    ]
);

const CONTEXT: &str = "GpuiCoreNavigationMenu";
const PANEL_CONTEXT: &str = "GpuiCoreNavigationMenuPanel";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    let p = Some(PANEL_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", PreviousItem, c),
        KeyBinding::new("right", NextItem, c),
        KeyBinding::new("down", OpenItem, c),
        KeyBinding::new("escape", CloseItem, c),
        KeyBinding::new("up", PreviousLink, p),
        KeyBinding::new("left", PreviousLink, p),
        KeyBinding::new("down", NextLink, p),
        KeyBinding::new("right", NextLink, p),
        KeyBinding::new("home", FirstLink, p),
        KeyBinding::new("end", LastLink, p),
    ]);
}

type OnNavigate = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// Opens a panel by its index (with the keyboard going into it, or not), or closes the open one.
type SetOpen = Rc<dyn Fn(Option<usize>, bool, &mut App)>;

/// A link in a panel: a title, and a line or two describing where it goes.
pub struct NavigationMenuLink {
    id: SharedString,
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<IconName>,
}

impl NavigationMenuLink {
    /// `id` is what [`NavigationMenu::on_navigate`] is told when it's followed.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            icon: None,
        }
    }

    /// Under the title, muted, cut short after two lines.
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// An icon before the title.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

enum Kind {
    Link,
    Panel {
        links: Vec<NavigationMenuLink>,
        columns: u16,
    },
}

/// An item in the row: a link, or a trigger with a panel of links.
pub struct NavigationMenuItem {
    id: SharedString,
    label: SharedString,
    kind: Kind,
}

impl NavigationMenuItem {
    /// A link in the row itself.
    pub fn link(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: Kind::Link,
        }
    }

    /// A trigger that opens a panel of [`link`](Self::link)s.
    pub fn panel(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: Kind::Panel {
                links: Vec::new(),
                columns: 1,
            },
        }
    }

    /// A link in its panel (an item made with [`panel`](Self::panel)).
    pub fn with_link(mut self, link: NavigationMenuLink) -> Self {
        if let Kind::Panel { links, .. } = &mut self.kind {
            links.push(link);
        }
        self
    }

    /// How many columns its panel's links are laid out in (one unless given). A window too
    /// narrow for them gets fewer.
    pub fn columns(mut self, columns: u16) -> Self {
        if let Kind::Panel { columns: c, .. } = &mut self.kind {
            *c = columns.max(1);
        }
        self
    }
}

/// A row of links and of triggers opening panels of links: a navigation landmark.
#[derive(IntoElement)]
pub struct NavigationMenu {
    id: ElementId,
    items: Vec<NavigationMenuItem>,
    on_navigate: Option<OnNavigate>,
    label: SharedString,
    touch: bool,
    style: StyleRefinement,
}

impl NavigationMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            on_navigate: None,
            label: "Main".into(),
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn item(mut self, item: NavigationMenuItem) -> Self {
        self.items.push(item);
        self
    }

    /// Told the link followed, in the row or in a panel; an open panel closes.
    pub fn on_navigate(
        mut self,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(f));
        self
    }

    /// What it is to assistive tech ("Main" unless given).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Triggers and links 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for NavigationMenu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// What a navigation menu remembers between frames.
#[derive(Default)]
struct NavState {
    items: Vec<FocusHandle>,
    /// The open panel's links'.
    links: Vec<FocusHandle>,
    open: Option<SharedString>,
    /// Opened from the keyboard and not yet given it: its first link takes it when drawn.
    focus_first: bool,
}

/// The next index from `from`, `step` at a time, wrapping, among `n`.
fn wrap(n: usize, from: usize, step: isize) -> usize {
    (from as isize + step).rem_euclid(n.max(1) as isize) as usize
}

/// An item in the row: a trigger (expanded while its panel's open) or a link.
fn item_press(
    nav: &ElementId,
    item: &NavigationMenuItem,
    open: bool,
    focus: &FocusHandle,
    touch: bool,
    theme: &Theme,
) -> Press {
    let selector = format!("nav-{}", item.id);
    let panel = matches!(item.kind, Kind::Panel { .. });
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(nav.clone()),
        item.id.clone(),
    ))
    .role(if panel { Role::Button } else { Role::Link })
    .when(panel, |p| p.aria_expanded(open))
    .accessibility_label(item.label.clone())
    .track_focus(focus)
    .debug_selector(move || selector.clone())
    .flex()
    .flex_none()
    .items_center()
    .gap(css(4.))
    .h(css(36.))
    .when(touch, |p| p.h(theme.touch_target))
    .px(css(16.))
    .rounded(theme.radius_md())
    .text_size(theme.text.sm)
    .font_weight(gpui::FontWeight::MEDIUM)
    .whitespace_nowrap()
    .when(open, |p| p.bg(theme.accent.opacity(0.5)))
    .hover({
        let (bg, fg) = (theme.accent, theme.accent_foreground);
        move |s| s.bg(bg).text_color(fg)
    })
    .child(item.label.clone())
    .when(panel, |p| {
        p.child(
            Icon::new(if open {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .size(css(12.)),
        )
    })
}

/// A link in a panel: its title over its description.
fn panel_link(
    nav: &ElementId,
    link: &NavigationMenuLink,
    focus: &FocusHandle,
    touch: bool,
    theme: &Theme,
) -> Press {
    let selector = format!("nav-link-{}", link.id);
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(nav.clone()),
        format!("link-{}", link.id).into(),
    ))
    .role(Role::Link)
    .accessibility_label(link.title.clone())
    .track_focus(focus)
    .debug_selector(move || selector.clone())
    .flex()
    .flex_col()
    .gap(css(4.))
    .min_w_0()
    .p(css(8.))
    .when(touch, |p| p.min_h(theme.touch_target))
    .rounded(theme.radius_sm())
    .text_size(theme.text.sm)
    .hover({
        let (bg, fg) = (theme.accent, theme.accent_foreground);
        move |s| s.bg(bg).text_color(fg)
    })
    .focus({
        let bg = theme.accent;
        move |s| s.bg(bg)
    })
    .child(
        div()
            .flex()
            .items_center()
            .gap(css(8.))
            .min_w_0()
            .font_weight(gpui::FontWeight::MEDIUM)
            .when_some(link.icon, |d, i| d.child(Icon::new(i)))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(link.title.clone()),
            ),
    )
    .when_some(link.description.clone(), |p, d| {
        p.child(
            div()
                .text_color(theme.muted_foreground)
                .line_clamp(2)
                .child(d),
        )
    })
}

impl RenderOnce for NavigationMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let theme = cx.theme().clone();
        let n = self.items.len();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| NavState::default());
        let ids: Rc<Vec<SharedString>> = Rc::new(self.items.iter().map(|i| i.id.clone()).collect());
        let is_panel: Rc<Vec<bool>> = Rc::new(
            self.items
                .iter()
                .map(|i| matches!(i.kind, Kind::Panel { .. }))
                .collect(),
        );
        let open = state
            .read(cx)
            .open
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id))
            .filter(|i| is_panel[*i]);
        let open_links = open
            .and_then(|i| match &self.items[i].kind {
                Kind::Panel { links, .. } => Some(links.len()),
                Kind::Link => None,
            })
            .unwrap_or(0);
        let focus_first = state.update(cx, |s, cx| {
            while s.items.len() < n {
                s.items.push(cx.focus_handle());
            }
            s.items.truncate(n);
            while s.links.len() < open_links {
                s.links.push(cx.focus_handle());
            }
            s.links.truncate(open_links);
            std::mem::take(&mut s.focus_first)
        });
        let (items, links) = {
            let s = state.read(cx);
            (Rc::new(s.items.clone()), Rc::new(s.links.clone()))
        };
        if focus_first && let Some(first) = links.first().cloned() {
            // After the popover has drawn, so the link is in the window to take it.
            window.defer(cx, move |window, cx| window.focus(&first, cx));
        }

        // Opens panel `to` (or closes the open one); `keys`: the keyboard goes into it.
        let set_open: SetOpen = Rc::new({
            let (state, ids) = (state.clone(), ids.clone());
            move |to, keys, cx| {
                state.update(cx, |s, cx| {
                    let id = to.map(|i| ids[i].clone());
                    if s.open != id {
                        s.open = id;
                        s.focus_first = keys && to.is_some();
                        cx.notify();
                    }
                })
            }
        });
        let navigate: OnNavigate = Rc::new({
            let (on_navigate, set_open) = (self.on_navigate.clone(), set_open.clone());
            move |id, window, cx| {
                set_open(None, false, cx);
                if let Some(f) = &on_navigate {
                    f(id, window, cx);
                }
            }
        });

        // Left and Right in the row: the keyboard moves to the item beside; with a panel open,
        // that item's panel opens if it has one.
        let mover = |delta: isize| {
            let (items, set_open, is_panel) = (items.clone(), set_open.clone(), is_panel.clone());
            move |window: &mut Window, cx: &mut App| {
                let Some(from) = items.iter().position(|h| h.is_focused(window)).or(open) else {
                    return;
                };
                let to = wrap(items.len(), from, delta);
                window.focus(&items[to], cx);
                if open.is_some() {
                    set_open(is_panel[to].then_some(to), false, cx);
                }
            }
        };
        let (prev_item, next_item) = (mover(-1), mover(1));
        let open_item = {
            let (items, set_open, is_panel) = (items.clone(), set_open.clone(), is_panel.clone());
            move |window: &mut Window, cx: &mut App| match items
                .iter()
                .position(|h| h.is_focused(window))
            {
                Some(i) if is_panel[i] => set_open(Some(i), true, cx),
                _ => cx.propagate(),
            }
        };
        let close = {
            let set_open = set_open.clone();
            move |_: &CloseItem, _: &mut Window, cx: &mut App| {
                if open.is_some() {
                    set_open(None, false, cx);
                } else {
                    cx.propagate();
                }
            }
        };
        let link_mover = |to: fn(usize, usize) -> usize| {
            let links = links.clone();
            move |window: &mut Window, cx: &mut App| {
                let n = links.len();
                if n == 0 {
                    return;
                }
                let at = links.iter().position(|h| h.is_focused(window));
                let i = match at {
                    Some(at) => to(n, at),
                    None => 0,
                };
                window.focus(&links[i], cx);
            }
        };
        let prev_link = link_mover(|n, at| wrap(n, at, -1));
        let next_link = link_mover(|n, at| wrap(n, at, 1));
        let first_link = link_mover(|_, _| 0);
        let last_link = link_mover(|n, _| n - 1);
        let link_keys = Rc::new((prev_link, next_link, first_link, last_link));

        // A panel's columns, as many as fit the window, and its grid's width.
        let rem = window.rem_size();
        let link_w = css(200.).to_pixels(rem);
        let gap = css(4.).to_pixels(rem);
        let room = window.viewport_size().width - css(2. * 8. + 2. * 8. + 2.).to_pixels(rem);
        let fit = move |columns: u16| {
            let most = (((room + gap) / (link_w + gap)).floor() as u16).max(1);
            let cols = columns.min(most);
            let width =
                (link_w * cols as f32 + gap * (cols as f32 - 1.)).min(room.max(link_w / 2.));
            (cols, width)
        };

        let touch = self.touch;
        let row = self.items.into_iter().enumerate().map(|(i, item)| {
            let is_open = open == Some(i);
            let press = item_press(&self.id, &item, is_open, &items[i], touch, &theme);
            match item.kind {
                Kind::Link => {
                    let navigate = navigate.clone();
                    let id = item.id.clone();
                    press
                        .on_click(move |_, window, cx| navigate(&id, window, cx))
                        .into_any_element()
                }
                Kind::Panel {
                    links: list,
                    columns,
                } => {
                    let (toggle, hover, dismiss) =
                        (set_open.clone(), set_open.clone(), set_open.clone());
                    let trigger = press
                        .on_hover(move |hovered, _, cx| {
                            if *hovered && open.is_some() && open != Some(i) {
                                hover(Some(i), false, cx);
                            }
                        })
                        .on_click(move |e, _, cx| {
                            toggle((!is_open).then_some(i), e.is_keyboard(), cx)
                        });
                    let content = is_open.then(|| {
                        let keys = link_keys.clone();
                        let (k1, k2, k3, k4) = (keys.clone(), keys.clone(), keys.clone(), keys);
                        div()
                            .id(ElementId::NamedChild(
                                std::sync::Arc::new(self.id.clone()),
                                format!("{}-links", item.id).into(),
                            ))
                            .key_context(PANEL_CONTEXT)
                            .on_action(move |_: &PreviousLink, w, cx| (k1.0)(w, cx))
                            .on_action(move |_: &NextLink, w, cx| (k2.1)(w, cx))
                            .on_action(move |_: &FirstLink, w, cx| (k3.2)(w, cx))
                            .on_action(move |_: &LastLink, w, cx| (k4.3)(w, cx))
                            .debug_selector({
                                let id = item.id.clone();
                                move || format!("nav-panel-{id}")
                            })
                            .grid()
                            .grid_cols(fit(columns).0)
                            .gap(css(4.))
                            .w(fit(columns).1)
                            .children(list.iter().enumerate().map(|(j, l)| {
                                let navigate = navigate.clone();
                                let id = l.id.clone();
                                panel_link(&self.id, l, &links[j], touch, &theme)
                                    .on_click(move |_, window, cx| navigate(&id, window, cx))
                            }))
                    });
                    Popover::new(ElementId::NamedChild(
                        std::sync::Arc::new(self.id.clone()),
                        format!("{}-panel", item.id).into(),
                    ))
                    .open(is_open)
                    .take_focus(false)
                    .return_focus(&items[i])
                    .accessibility_label(item.label.clone())
                    .match_trigger_width(false)
                    .max_height(rems(30.))
                    .on_dismiss(move |_, _, cx| dismiss(None, false, cx))
                    .trigger(trigger)
                    .p(css(8.))
                    .children(content)
                    .into_any_element()
                }
            }
        });

        let mut nav = div()
            .id(self.id.clone())
            .role(Role::Navigation)
            .aria_label(self.label.clone())
            .debug_selector(|| "navigation-menu".into())
            .key_context(CONTEXT)
            .on_action(move |_: &PreviousItem, w, cx| prev_item(w, cx))
            .on_action(move |_: &NextItem, w, cx| next_item(w, cx))
            .on_action(move |_: &OpenItem, w, cx| open_item(w, cx))
            .on_action(close)
            .flex()
            .items_center()
            .gap(css(4.))
            .min_w_0()
            .max_w_full()
            .overflow_x_hidden()
            .p(css(3.))
            .children(row);
        gpui::Refineable::refine(nav.style(), &self.style);
        nav
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    #[gpui::test]
    fn triggers_and_links_have_their_roles(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let theme = cx.theme().clone();
            let focus = cx.focus_handle();
            let id = ElementId::from("n");
            let role = |p: Press, window: &mut Window, cx: &mut App| {
                let el = p.render(window, cx).into_element();
                let mut node = accesskit::Node::new(Role::Unknown);
                el.write_a11y_info(&mut node);
                (el.a11y_role(), node)
            };
            let item = NavigationMenuItem::panel("c", "Components");
            let (r, node) = role(
                item_press(&id, &item, true, &focus, false, &theme),
                window,
                cx,
            );
            assert_eq!(r, Some(Role::Button));
            assert_eq!(node.is_expanded(), Some(true));
            assert_eq!(node.label(), Some("Components"));
            let item = NavigationMenuItem::link("d", "Docs");
            let (r, node) = role(
                item_press(&id, &item, false, &focus, false, &theme),
                window,
                cx,
            );
            assert_eq!(r, Some(Role::Link));
            assert_eq!(node.is_expanded(), None);
            let link = NavigationMenuLink::new("t", "Tabs").description("Panels");
            let (r, node) = role(panel_link(&id, &link, &focus, false, &theme), window, cx);
            assert_eq!(r, Some(Role::Link));
            assert_eq!(node.label(), Some("Tabs"));
        });
    }
}
