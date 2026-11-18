//! A sidebar, as shadcn's Sidebar: an app's side navigation beside its content, with a header,
//! groups of items (an icon, a label, a badge, an action) that can fold away, and a footer.
//!
//! It collapses to a rail of icons (each with its label as a tooltip) with a
//! [`SidebarTrigger`] or ⌘B (Ctrl-B off Apple), and in a window narrower than its breakpoint it
//! leaves the content the whole width and opens over it instead, from the left over a scrim.
//! The host keeps a [`SidebarState`], which says which.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{IconName, Sidebar, SidebarGroup, SidebarItem, SidebarState, SidebarTrigger};
//! # struct App { sidebar: Entity<SidebarState>, page: SharedString }
//! # impl Render for App {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Sidebar::new("nav", &self.sidebar)
//!         .group(
//!             SidebarGroup::new("main")
//!                 .label("Platform")
//!                 .item(SidebarItem::new("inbox", "Inbox").icon(IconName::Inbox).badge("4"))
//!                 .item(SidebarItem::new("agents", "Agents").icon(IconName::Bot)),
//!         )
//!         .active(self.page.clone())
//!         .on_select(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.page = id.clone();
//!             cx.notify();
//!         }))
//!         .content(
//!             div()
//!                 .child(SidebarTrigger::new("toggle", &self.sidebar))
//!                 .child(self.page.clone()),
//!         )
//! }
//! # }
//! ```

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, ElementId, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyBinding, MouseButton, ParentElement as _, Pixels, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, accesskit::AriaCurrent,
    actions, anchored, deferred, div, hsla, point, prelude::FluentBuilder as _, px, rems,
};

use crate::base::{
    ActiveTheme as _, Current, Disableable as _, Dismiss, Icon, IconName, OVERLAY_PRIORITY,
    POPOVER_CONTEXT as OVERLAY_CONTEXT, Press, Theme, overlay_focus,
};
use crate::tooltip::Tooltip;

actions!(
    gpui_core_sidebar,
    [
        /// Collapses or expands the sidebar, or opens or closes it over a narrow window.
        ToggleSidebar,
        /// The item above in the sidebar.
        PreviousItem,
        /// The item below.
        NextItem,
        FirstItem,
        LastItem
    ]
);

const CONTEXT: &str = "GpuiCoreSidebar";
const MENU_CONTEXT: &str = "GpuiCoreSidebarMenu";

pub(crate) fn init(cx: &mut App) {
    let toggle = if cfg!(any(target_os = "macos", target_os = "ios")) {
        "cmd-b"
    } else {
        "ctrl-b"
    };
    let m = Some(MENU_CONTEXT);
    cx.bind_keys([
        KeyBinding::new(toggle, ToggleSidebar, Some(CONTEXT)),
        KeyBinding::new("up", PreviousItem, m),
        KeyBinding::new("down", NextItem, m),
        KeyBinding::new("home", FirstItem, m),
        KeyBinding::new("end", LastItem, m),
    ]);
}

/// Whether a sidebar is collapsed to its rail, open over a narrow window, and which of its
/// groups are folded. The host makes one (`cx.new(|_| SidebarState::new())`), hands it to its
/// [`Sidebar`] and [`SidebarTrigger`], and can observe it to keep the choice.
pub struct SidebarState {
    collapsed: bool,
    open: bool,
    narrow: bool,
    folded: HashSet<SharedString>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarState {
    /// Expanded, with every group unfolded.
    pub fn new() -> Self {
        Self {
            collapsed: false,
            open: false,
            narrow: false,
            folded: HashSet::new(),
        }
    }

    /// Whether it's collapsed to its rail (in a wide window).
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed != collapsed {
            self.collapsed = collapsed;
            cx.notify();
        }
    }

    /// Whether it's open over the content (in a narrow window).
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open != open {
            self.open = open;
            cx.notify();
        }
    }

    /// Whether the window was narrower than the breakpoint when it was last drawn.
    pub fn is_narrow(&self) -> bool {
        self.narrow
    }

    /// What its trigger and ⌘B do: in a narrow window, open or close it over the content;
    /// otherwise collapse or expand it.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.narrow {
            self.set_open(!self.open, cx);
        } else {
            self.set_collapsed(!self.collapsed, cx);
        }
    }

    /// Whether group `id` is folded away.
    pub fn is_folded(&self, id: &str) -> bool {
        self.folded.contains(id)
    }

    pub fn set_folded(
        &mut self,
        id: impl Into<SharedString>,
        folded: bool,
        cx: &mut Context<Self>,
    ) {
        let id = id.into();
        let changed = if folded {
            self.folded.insert(id)
        } else {
            self.folded.remove(&id)
        };
        if changed {
            cx.notify();
        }
    }
}

type OnItem = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// Folds or unfolds a group.
type FoldFn = Rc<dyn Fn(&mut App)>;

/// An item: an icon, a label, and maybe a badge and an action.
pub struct SidebarItem {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
    badge: Option<SharedString>,
    action: Option<(IconName, SharedString, OnItem)>,
    disabled: bool,
}

impl SidebarItem {
    /// `id` is what [`Sidebar::on_select`] is told and [`Sidebar::active`] takes.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            badge: None,
            action: None,
            disabled: false,
        }
    }

    /// Its icon, which is all the rail shows of it. Give every item one.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A count or a word at its end ("24"); hidden in the rail.
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// A button of its own at its end (an icon, labelled `label` to assistive tech), told the
    /// item's id when pressed; hidden in the rail.
    pub fn action(
        mut self,
        icon: IconName,
        label: impl Into<SharedString>,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((icon, label.into(), Rc::new(f)));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// A group of items under a label, which can fold them away when it's
/// [`collapsible`](Self::collapsible).
pub struct SidebarGroup {
    id: SharedString,
    label: Option<SharedString>,
    collapsible: bool,
    items: Vec<SidebarItem>,
}

impl SidebarGroup {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: None,
            collapsible: false,
            items: Vec::new(),
        }
    }

    /// Its title, muted, above its items; hidden in the rail.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Whether a press on its label (or Enter or Space on it) folds its items away and back.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    pub fn item(mut self, item: SidebarItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = SidebarItem>) -> Self {
        self.items.extend(items);
        self
    }
}

/// The sidebar and the content beside it, filling the box it's in (the window, usually).
#[derive(IntoElement)]
pub struct Sidebar {
    id: ElementId,
    state: Entity<SidebarState>,
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    groups: Vec<SidebarGroup>,
    content: Option<AnyElement>,
    active: Option<SharedString>,
    on_select: Option<OnItem>,
    breakpoint: Pixels,
    label: SharedString,
    touch: bool,
    style: StyleRefinement,
}

impl Sidebar {
    pub fn new(id: impl Into<ElementId>, state: &Entity<SidebarState>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            header: None,
            footer: None,
            groups: Vec::new(),
            content: None,
            active: None,
            on_select: None,
            breakpoint: px(768.),
            label: "Sidebar".into(),
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// Above the groups (a team switcher, the app's name); hidden in the rail.
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Below the groups, at the bottom (the signed-in user); hidden in the rail.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub fn group(mut self, group: SidebarGroup) -> Self {
        self.groups.push(group);
        self
    }

    /// What's beside the sidebar: the page.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    /// The item for the page that's shown: drawn highlighted, and the current page to
    /// assistive tech.
    pub fn active(mut self, id: impl Into<SharedString>) -> Self {
        self.active = Some(id.into());
        self
    }

    /// Told the item pressed. In a narrow window the sidebar then closes.
    pub fn on_select(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The window width under which it opens over the content instead of beside it (768
    /// pixels unless given, the web's `md`).
    pub fn breakpoint(mut self, width: Pixels) -> Self {
        self.breakpoint = width;
        self
    }

    /// What it is to assistive tech ("Sidebar" unless given).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Items 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

/// The sidebar's own style (its width, its colours); the content beside it is the host's.
impl Styled for Sidebar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// The rail's width, the expanded sidebar's, and the narrow window's sheet's, in CSS pixels.
const RAIL: f32 = 48.;
const WIDE: f32 = 256.;
const SHEET: f32 = 288.;

/// How an item is drawn.
#[derive(Clone, Copy)]
struct Look {
    rail: bool,
    touch: bool,
}

/// An item's button: a link to assistive tech, the current page when it's the active one.
fn item_press(
    sidebar: &ElementId,
    item: &SidebarItem,
    active: bool,
    focus: &FocusHandle,
    look: Look,
    theme: &Theme,
) -> Press {
    let id = item.id.clone();
    let label = item.label.clone();
    let (fg, accent, accent_fg) = (
        theme.sidebar_foreground,
        theme.sidebar_accent,
        theme.sidebar_accent_foreground,
    );
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(sidebar.clone()),
        item.id.clone(),
    ))
    .role(Role::Link)
    .accessibility_label(item.label.clone())
    .track_focus(focus)
    .tab_stop(true)
    .debug_selector(move || format!("sidebar-item-{id}"))
    .flex()
    .items_center()
    .gap(css(8.))
    .min_w_0()
    .rounded(theme.radius_md())
    .p(css(8.))
    .text_size(theme.text.sm)
    .text_color(fg)
    .map(|p| {
        if look.rail {
            p.size(css(32.)).flex_none().justify_center()
        } else {
            p.w_full().h(css(32.))
        }
    })
    .when(look.touch, |p| {
        p.h(theme.touch_target)
            .when(look.rail, |p| p.w(theme.touch_target))
    })
    .when(active, |p| {
        p.bg(accent)
            .text_color(accent_fg)
            .font_weight(gpui::FontWeight::MEDIUM)
    })
    .when(!item.disabled, |p| {
        p.hover(move |s| s.bg(accent).text_color(accent_fg))
    })
    .when(item.disabled, |p| p.opacity(0.5))
    .when(look.rail, |p| {
        // The rail shows only icons: the label is a tooltip (and the accessibility label).
        p.tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
    })
    .when_some(item.icon, |p, i| p.child(Icon::new(i)))
    .when(!look.rail, |p| {
        let id = item.id.clone();
        p.child(
            div()
                .debug_selector(move || format!("sidebar-label-{id}"))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(item.label.clone()),
        )
        .when_some(item.badge.clone(), |p, b| {
            p.child(
                div()
                    .flex_none()
                    .min_w(css(20.))
                    .h(css(20.))
                    .px(css(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radius_md())
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(b),
            )
        })
        // Room for the action drawn over the row's end.
        .when(item.action.is_some(), |p| p.pr(css(32.)))
    })
}

/// An item's press, the current page to assistive tech when it's the active item.
fn current(
    press: Press,
    active: bool,
    window: &mut Window,
    cx: &mut App,
) -> Current<impl gpui::Element> {
    let el = press.render(window, cx).into_element();
    Current::new(el, active.then_some(AriaCurrent::Page))
}

/// A group's label's look, on a div or a Press.
fn label_look<T: Styled>(d: T, theme: &Theme) -> T {
    d.flex()
        .items_center()
        .gap(css(4.))
        .h(css(32.))
        .px(css(8.))
        .min_w_0()
        .rounded(theme.radius_md())
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.sidebar_foreground.opacity(0.7))
}

/// A group's label: a button that folds the group when it's collapsible (`fold` given).
fn group_label(
    sidebar: &ElementId,
    group: &SidebarGroup,
    label: SharedString,
    folded: bool,
    fold: Option<FoldFn>,
    touch: bool,
    theme: &Theme,
) -> AnyElement {
    let id = group.id.clone();
    let title = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .child(label.clone());
    let Some(fold) = fold else {
        return label_look(div(), theme)
            .debug_selector(move || format!("sidebar-group-label-{id}"))
            .child(title)
            .into_any_element();
    };
    label_look(
        Press::new(ElementId::NamedChild(
            std::sync::Arc::new(sidebar.clone()),
            format!("{}-label", group.id).into(),
        )),
        theme,
    )
    .accessibility_label(label)
    .aria_expanded(!folded)
    .debug_selector(move || format!("sidebar-group-label-{id}"))
    .w_full()
    .when(touch, |p| p.h(theme.touch_target))
    .hover({
        let (bg, fg) = (theme.sidebar_accent, theme.sidebar_accent_foreground);
        move |s| s.bg(bg).text_color(fg)
    })
    .child(title)
    .child(Icon::new(if folded {
        IconName::ChevronRight
    } else {
        IconName::ChevronDown
    }))
    .on_click(move |_, _, cx| fold(cx))
    .into_any_element()
}

/// What a sidebar remembers between frames: its items' focus handles, by id.
#[derive(Default)]
struct FocusState {
    items: Vec<(SharedString, FocusHandle)>,
}

impl RenderOnce for Sidebar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let narrow = window.viewport_size().width < self.breakpoint;
        let (collapsed, open, folded) = self.state.update(cx, |s, _| {
            s.narrow = narrow;
            if !narrow {
                s.open = false;
            }
            (s.collapsed, s.open, s.folded.clone())
        });
        let rail = collapsed && !narrow;
        let look = Look {
            rail,
            touch: self.touch,
        };

        // A focus handle for each item drawn, kept by id.
        let shown: Vec<SharedString> = self
            .groups
            .iter()
            .filter(|g| rail || !folded.contains(&g.id))
            .flat_map(|g| g.items.iter().map(|i| i.id.clone()))
            .collect();
        let enabled: HashSet<SharedString> = self
            .groups
            .iter()
            .flat_map(|g| g.items.iter().filter(|i| !i.disabled).map(|i| i.id.clone()))
            .collect();
        let focus = window.use_keyed_state(self.id.clone(), cx, |_, _| FocusState::default());
        let handles: Rc<Vec<(SharedString, FocusHandle)>> = Rc::new(focus.update(cx, |f, cx| {
            let mut next = Vec::with_capacity(shown.len());
            for id in &shown {
                let h = f
                    .items
                    .iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, h)| h.clone())
                    .unwrap_or_else(|| cx.focus_handle());
                next.push((id.clone(), h));
            }
            f.items = next.clone();
            next
        }));
        let handle = |id: &SharedString| {
            handles
                .iter()
                .find(|(i, _)| i == id)
                .map(|(_, h)| h.clone())
                .expect("a handle for every item drawn")
        };
        // The items the arrows move between: those drawn that can be pressed.
        let keyed: Rc<Vec<FocusHandle>> = Rc::new(
            handles
                .iter()
                .filter(|(id, _)| enabled.contains(id))
                .map(|(_, h)| h.clone())
                .collect(),
        );

        let state = self.state.clone();
        let select: OnItem = Rc::new({
            let (on_select, state) = (self.on_select.clone(), state.clone());
            move |id, window, cx| {
                state.update(cx, |s, cx| {
                    if s.narrow {
                        s.set_open(false, cx);
                    }
                });
                if let Some(f) = &on_select {
                    f(id, window, cx);
                }
            }
        });
        let mover = |to: fn(usize, Option<usize>) -> usize| {
            let keyed = keyed.clone();
            move |window: &mut Window, cx: &mut App| {
                let n = keyed.len();
                if n == 0 {
                    return;
                }
                let at = keyed.iter().position(|h| h.is_focused(window));
                window.focus(&keyed[to(n, at)], cx);
            }
        };
        let prev = mover(|n, at| at.map_or(n - 1, |a| a.saturating_sub(1)));
        let next = mover(|n, at| at.map_or(0, |a| (a + 1).min(n - 1)));
        let first = mover(|_, _| 0);
        let last = mover(|n, _| n - 1);

        let groups = self
            .groups
            .into_iter()
            .map(|group| {
                let is_folded = !rail && group.collapsible && folded.contains(&group.id);
                let label = group.label.clone().filter(|_| !rail).map(|l| {
                    let fold: Option<FoldFn> = group.collapsible.then(|| {
                        let (state, gid) = (state.clone(), group.id.clone());
                        Rc::new(move |cx: &mut App| {
                            state.update(cx, |s, cx| {
                                let f = s.is_folded(&gid);
                                s.set_folded(gid.clone(), !f, cx)
                            })
                        }) as FoldFn
                    });
                    group_label(&self.id, &group, l, is_folded, fold, self.touch, &theme)
                });
                let items = (!is_folded).then(|| {
                    group
                        .items
                        .iter()
                        .map(|item| {
                            let active = self.active.as_ref() == Some(&item.id);
                            let select = select.clone();
                            let id = item.id.clone();
                            let press =
                                item_press(&self.id, item, active, &handle(&item.id), look, &theme)
                                    .when(!item.disabled, |p| {
                                        p.on_click(move |_, window, cx| select(&id, window, cx))
                                    })
                                    .disabled(item.disabled);
                            let row = current(press, active, window, cx);
                            let action =
                                item.action
                                    .as_ref()
                                    .filter(|_| !rail)
                                    .map(|(icon, label, f)| {
                                        let (f, id) = (f.clone(), item.id.clone());
                                        let sel = format!("sidebar-action-{}", item.id);
                                        Press::new(ElementId::NamedChild(
                                            std::sync::Arc::new(self.id.clone()),
                                            format!("{}-action", item.id).into(),
                                        ))
                                        .accessibility_label(label.clone())
                                        .debug_selector(move || sel.clone())
                                        .absolute()
                                        .right(css(4.))
                                        .top(css(6.))
                                        .when(self.touch, |p| p.top(css(12.)))
                                        .size(css(20.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(theme.radius_sm())
                                        .text_color(theme.sidebar_foreground)
                                        .hover({
                                            let (bg, fg) = (
                                                theme.sidebar_accent,
                                                theme.sidebar_accent_foreground,
                                            );
                                            move |s| s.bg(bg).text_color(fg)
                                        })
                                        .child(Icon::new(*icon))
                                        .on_click(move |_, window, cx| f(&id, window, cx))
                                    });
                            div()
                                .relative()
                                .min_w_0()
                                .when(rail, |d| d.flex().justify_center())
                                .child(row)
                                .children(action)
                        })
                        .collect::<Vec<_>>()
                });
                let gid = group.id.clone();
                div()
                    .id(ElementId::NamedChild(
                        std::sync::Arc::new(self.id.clone()),
                        format!("{}-group", group.id).into(),
                    ))
                    .role(Role::Group)
                    .when_some(group.label.clone(), |d, l| d.aria_label(l))
                    .debug_selector(move || format!("sidebar-group-{gid}"))
                    .flex()
                    .flex_col()
                    .gap(css(4.))
                    .p(css(8.))
                    .min_w_0()
                    .children(label)
                    .children(items.into_iter().flatten())
            })
            .collect::<Vec<_>>();

        let panel_id = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "panel".into());
        let mut panel = div()
            .id(panel_id)
            .role(Role::Navigation)
            .aria_label(self.label.clone())
            .debug_selector(|| "sidebar".into())
            .key_context(MENU_CONTEXT)
            .on_action(move |_: &PreviousItem, w, cx| prev(w, cx))
            .on_action(move |_: &NextItem, w, cx| next(w, cx))
            .on_action(move |_: &FirstItem, w, cx| first(w, cx))
            .on_action(move |_: &LastItem, w, cx| last(w, cx))
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .w(css(if narrow {
                SHEET
            } else if rail {
                RAIL
            } else {
                WIDE
            }))
            .max_w_full()
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .when_some(self.header.filter(|_| !rail), |d, h| {
                d.child(div().flex_none().p(css(8.)).min_w_0().child(h))
            })
            .child(
                div()
                    .id("sidebar-groups")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .children(groups),
            )
            .when_some(self.footer.filter(|_| !rail), |d, f| {
                d.child(div().flex_none().p(css(8.)).min_w_0().child(f))
            });
        gpui::Refineable::refine(panel.style(), &self.style);

        let toggle = {
            let state = state.clone();
            move |_: &ToggleSidebar, _: &mut Window, cx: &mut App| {
                state.update(cx, |s, cx| s.toggle(cx))
            }
        };
        let overlay_id =
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "sheet".into());
        let sheet_open = narrow && open;
        let sheet_focus = overlay_focus(&overlay_id, sheet_open, true, None, window, cx);
        // Opened over a narrow window: the keyboard goes to its first item.
        if sheet_open && let Some(h) = keyed.first().cloned() {
            let first_open = !sheet_focus.contains_focused(window, cx);
            if first_open {
                window.defer(cx, move |window, cx| {
                    if !h.is_focused(window) {
                        window.focus(&h, cx)
                    }
                });
            }
        }
        let view = window.viewport_size();
        let close = {
            let state = state.clone();
            move |cx: &mut App| state.update(cx, |s, cx| s.set_open(false, cx))
        };
        let close_scrim = close.clone();
        let mut row = div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .on_action(toggle)
            .relative()
            .flex()
            .size_full()
            .min_w_0()
            .min_h_0();
        let mut panel = Some(panel);
        if !narrow {
            row = row.children(panel.take());
        }
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .h_full()
                .children(self.content),
        );
        if sheet_open {
            row = row.child(
                deferred(
                    anchored().position(point(px(0.), px(0.))).child(
                        div()
                            .id(overlay_id)
                            .occlude()
                            .w(view.width)
                            .h(view.height)
                            .bg(hsla(0., 0., 0., 0.5))
                            .debug_selector(|| "sidebar-scrim".into())
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| close_scrim(cx))
                            .child(
                                div()
                                    .id("sidebar-sheet")
                                    .key_context(OVERLAY_CONTEXT)
                                    .track_focus(&sheet_focus)
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_action(move |_: &Dismiss, _, cx| close(cx))
                                    .h_full()
                                    .w(css(SHEET))
                                    .max_w_full()
                                    .shadow_lg()
                                    .children(panel.take()),
                            ),
                    ),
                )
                .with_priority(OVERLAY_PRIORITY),
            );
        }
        row
    }
}

/// A button that collapses and expands a [`Sidebar`] (or opens and closes it over a narrow
/// window): shadcn's SidebarTrigger, a small ghost button with the panel icon.
#[derive(IntoElement)]
pub struct SidebarTrigger {
    id: ElementId,
    state: Entity<SidebarState>,
    touch: bool,
}

impl SidebarTrigger {
    pub fn new(id: impl Into<ElementId>, state: &Entity<SidebarState>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            touch: false,
        }
    }

    /// 44 points each way, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl RenderOnce for SidebarTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let s = self.state.read(cx);
        let expanded = if s.narrow { s.open } else { !s.collapsed };
        let state = self.state.clone();
        let side = if self.touch {
            theme.touch_target.into()
        } else {
            gpui::DefiniteLength::from(css(28.))
        };
        let hover = if theme.mode.is_dark() {
            theme.accent.opacity(0.5)
        } else {
            theme.accent
        };
        Press::new(self.id)
            .accessibility_label("Toggle sidebar")
            .aria_expanded(expanded)
            .debug_selector(|| "sidebar-trigger".into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(side)
            .rounded(theme.radius_md())
            .text_color(theme.foreground)
            .hover(move |s| s.bg(hover))
            .child(Icon::new(IconName::PanelLeft))
            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle(cx)))
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    #[gpui::test]
    fn items_say_what_they_are_and_which_is_current(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let theme = cx.theme().clone();
            let focus = cx.focus_handle();
            let item = SidebarItem::new("inbox", "Inbox");
            for (active, rail) in [(true, false), (false, true)] {
                let look = Look { rail, touch: false };
                let press = item_press(&"s".into(), &item, active, &focus, look, &theme);
                let el = current(press, active, window, cx);
                let mut node = accesskit::Node::new(Role::Unknown);
                el.write_a11y_info(&mut node);
                assert_eq!(el.a11y_role(), Some(Role::Link));
                assert_eq!(node.label(), Some("Inbox"), "named in the rail too");
                assert_eq!(node.aria_current(), active.then_some(AriaCurrent::Page));
            }
        });
    }
}
