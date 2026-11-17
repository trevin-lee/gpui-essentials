//! Tabs: a row of tabs over the panel of the one that's selected (a settings page's Account
//! and Password). The row is a tab list to assistive tech with one Tab stop (the selected tab);
//! Left and Right move along it, wrapping, and Home and End go to its ends, passing over a
//! disabled tab. By default reaching a tab selects it; with [`Tabs::manual`] the arrows only
//! move the focus and Enter or Space selects. Only the selected tab's panel is drawn.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Tab, Tabs};
//! # struct Settings { tab: SharedString }
//! # impl Render for Settings {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Tabs::new("settings")
//!         .accessibility_label("Settings")
//!         .tab(Tab::new("account", "Account").child("Your name and email."))
//!         .tab(Tab::new("password", "Password").child("Change your password."))
//!         .selected(self.tab.clone())
//!         .on_change(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.tab = id.clone();
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement, RenderOnce, Role, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, actions, div,
    prelude::FluentBuilder as _, px, rems,
};

use crate::base::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Press, Selectable as _, Theme,
};

actions!(
    gpui_core_tabs,
    [
        /// The tab before the one with the keyboard (the last from the first).
        Previous,
        /// The tab after it (the first from the last).
        Next,
        First,
        Last
    ]
);

const CONTEXT: &str = "GpuiCoreTabs";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Previous, c),
        KeyBinding::new("right", Next, c),
        KeyBinding::new("home", First, c),
        KeyBinding::new("end", Last, c),
    ]);
}

type OnChange = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// Selects the tab at an index.
type SelectFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// How the tab list is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsVariant {
    /// shadcn's default: the tabs on a muted track, the selected one raised on the background.
    #[default]
    Default,
    /// No track: the selected tab underlined.
    Line,
}

/// What the tab list does when its tabs don't fit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabsOverflow {
    /// The tabs narrow and their labels are cut short with an ellipsis.
    #[default]
    Shrink,
    /// The tabs keep their width and the list scrolls sideways, keeping the tab the keys reach
    /// in view.
    Scroll,
}

/// One tab: its label, and its panel's content as children.
pub struct Tab {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
    disabled: bool,
    children: Vec<AnyElement>,
}

impl Tab {
    /// `id` is what [`Tabs::on_change`] is told and what [`Tabs::selected`] takes.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            disabled: false,
            children: Vec::new(),
        }
    }

    /// An icon before the label.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Shown dimmed and passed over: no press or key selects it.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The panel's content: drawn only while the tab is selected.
impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// A tab list and the selected tab's panel. The host either owns the selection
/// ([`selected`](Self::selected), changing it in [`on_change`](Self::on_change)) or leaves it to
/// the tabs ([`default_selected`](Self::default_selected), or the first tab that can be
/// selected), which then remember it and still tell `on_change`.
#[derive(IntoElement)]
pub struct Tabs {
    id: ElementId,
    tabs: Vec<Tab>,
    selected: Option<SharedString>,
    default_selected: Option<SharedString>,
    on_change: Option<OnChange>,
    variant: TabsVariant,
    overflow: TabsOverflow,
    manual: bool,
    stretch: bool,
    label: Option<SharedString>,
    touch: bool,
    style: StyleRefinement,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            tabs: Vec::new(),
            selected: None,
            default_selected: None,
            on_change: None,
            variant: TabsVariant::Default,
            overflow: TabsOverflow::Shrink,
            manual: false,
            stretch: false,
            label: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn tab(mut self, tab: Tab) -> Self {
        self.tabs.push(tab);
        self
    }

    pub fn tabs(mut self, tabs: impl IntoIterator<Item = Tab>) -> Self {
        self.tabs.extend(tabs);
        self
    }

    /// The selected tab, owned by the host: a press or a key tells
    /// [`on_change`](Self::on_change), and the host makes it the selected one.
    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    /// The tab selected at first when the host leaves the selection to the tabs.
    pub fn default_selected(mut self, id: impl Into<SharedString>) -> Self {
        self.default_selected = Some(id.into());
        self
    }

    /// Told the tab a press or a key selected.
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    pub fn variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The underlined look: [`TabsVariant::Line`].
    pub fn line(self) -> Self {
        self.variant(TabsVariant::Line)
    }

    /// What the list does when its tabs don't fit: shrink them (the default) or scroll.
    pub fn overflow(mut self, overflow: TabsOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Whether the arrows only move the focus, leaving Enter or Space (or a press) to select
    /// (off by default: reaching a tab selects it). For panels that are slow to draw.
    pub fn manual(mut self, manual: bool) -> Self {
        self.manual = manual;
        self
    }

    /// Whether the list fills the width, its tabs sharing it (off by default: as wide as its
    /// tabs).
    pub fn stretch(mut self, stretch: bool) -> Self {
        self.stretch = stretch;
        self
    }

    /// What the tabs choose between, to assistive tech ("Settings").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Tabs at least 44 points each way, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

/// The whole element's style (the list and the panel together).
impl Styled for Tabs {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// The next tab from `from` that can be selected, `step` at a time, wrapping.
fn step(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
    let n = enabled.len() as isize;
    if n == 0 {
        return None;
    }
    let mut i = from as isize;
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        if enabled[i as usize] {
            return Some(i as usize);
        }
    }
    None
}

/// What the tabs remember between frames.
struct TabsState {
    focus: Vec<FocusHandle>,
    /// The selected tab, when the host leaves it to the tabs.
    selected: Option<SharedString>,
    scroll: ScrollHandle,
}

/// How a tab is drawn.
#[derive(Clone, Copy)]
struct Look {
    variant: TabsVariant,
    overflow: TabsOverflow,
    touch: bool,
}

/// A tab's button: a Tab to assistive tech, selected or not, labelled with its label.
fn trigger(
    list: &ElementId,
    tab: &Tab,
    selected: bool,
    tab_stop: bool,
    focus: &FocusHandle,
    look: Look,
    theme: &Theme,
) -> Press {
    let id = tab.id.clone();
    let text = if selected {
        theme.foreground
    } else {
        theme.muted_foreground
    };
    let line = look.variant == TabsVariant::Line;
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(list.clone()),
        tab.id.clone(),
    ))
    .role(Role::Tab)
    .selected(selected)
    .disabled(tab.disabled)
    .accessibility_label(tab.label.clone())
    .track_focus(focus)
    .tab_stop(tab_stop)
    .debug_selector(move || format!("tab-{id}"))
    .relative()
    .flex()
    .items_center()
    .justify_center()
    .gap(css(6.))
    .px(css(8.))
    .map(|p| match look.overflow {
        // As wide as its label, giving way when the row is short of room.
        TabsOverflow::Shrink => p.flex_auto().min_w_0(),
        TabsOverflow::Scroll => p.flex_none(),
    })
    .map(|p| {
        if look.touch {
            let t = theme.touch_target;
            p.min_h(t).min_w(t)
        } else {
            p.h_full()
        }
    })
    .rounded(theme.radius_md())
    .border_1()
    .border_color(gpui::transparent_black())
    .text_size(theme.text.sm)
    .font_weight(gpui::FontWeight::MEDIUM)
    .whitespace_nowrap()
    .text_color(text)
    .when(!selected && !tab.disabled, |p| {
        let hover = theme.foreground;
        p.hover(move |s| s.text_color(hover))
    })
    .when(selected && !line, |p| p.bg(theme.background).shadow_sm())
    .when(selected && line, |p| {
        // The underline, in the list's padding below the tab.
        p.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(-3.))
                .h(px(2.))
                .bg(theme.foreground),
        )
    })
    .when(tab.disabled, |p| p.opacity(0.5))
    .when_some(tab.icon, |p, icon| p.child(Icon::new(icon).color(text)))
    .child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .child(tab.label.clone()),
    )
}

/// The row the tabs sit in: a tab list to assistive tech.
fn list(
    id: &ElementId,
    label: Option<SharedString>,
    look: Look,
    theme: &Theme,
) -> Stateful<gpui::Div> {
    let line = look.variant == TabsVariant::Line;
    div()
        .id(ElementId::NamedChild(
            std::sync::Arc::new(id.clone()),
            "list".into(),
        ))
        .role(Role::TabList)
        .when_some(label, |d, l| d.aria_label(l))
        .debug_selector(|| "tab-list".into())
        .key_context(CONTEXT)
        .flex()
        .items_center()
        .min_w_0()
        .max_w_full()
        .p(px(3.))
        .when(!look.touch, |d| d.h(css(36.)))
        .map(|d| {
            if line {
                d.gap(css(4.))
            } else {
                d.gap(css(2.)).rounded(theme.radius_lg()).bg(theme.muted)
            }
        })
}

/// The selected tab's panel: a tab panel to assistive tech, named by its tab.
fn panel(list: &ElementId, tab: Tab) -> Stateful<gpui::Div> {
    let id = tab.id.clone();
    div()
        .id(ElementId::NamedChild(
            std::sync::Arc::new(list.clone()),
            format!("{}-panel", tab.id).into(),
        ))
        .role(Role::TabPanel)
        .aria_label(tab.label)
        .debug_selector(move || format!("tab-panel-{id}"))
        .w_full()
        .min_w_0()
        .flex_1()
        .children(tab.children)
}

impl RenderOnce for Tabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let n = self.tabs.len();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| TabsState {
            focus: Vec::new(),
            selected: None,
            scroll: ScrollHandle::new(),
        });
        state.update(cx, |s, cx| {
            while s.focus.len() < n {
                s.focus.push(cx.focus_handle());
            }
            s.focus.truncate(n);
        });
        let (handles, kept, scroll) = {
            let s = state.read(cx);
            (
                Rc::new(s.focus.clone()),
                s.selected.clone(),
                s.scroll.clone(),
            )
        };
        let ids: Rc<Vec<SharedString>> = Rc::new(self.tabs.iter().map(|t| t.id.clone()).collect());
        let enabled: Rc<Vec<bool>> = Rc::new(self.tabs.iter().map(|t| !t.disabled).collect());
        let at = |id: &SharedString| ids.iter().position(|i| i == id);
        let controlled = self.selected.is_some();
        // The selected tab: the host's; else the one picked here, the default, or the first
        // that can be selected.
        let on = match &self.selected {
            Some(id) => at(id),
            None => kept
                .as_ref()
                .or(self.default_selected.as_ref())
                .and_then(at)
                .filter(|i| enabled[*i])
                .or_else(|| enabled.iter().position(|e| *e)),
        };
        // The one Tab stop: the selected tab, else the first that can be.
        let stop = on
            .filter(|i| enabled[*i])
            .or_else(|| enabled.iter().position(|e| *e));

        // Selects tab `i`, telling the host if it changed.
        let select: SelectFn = Rc::new({
            let (ids, state, on_change) = (ids.clone(), state.clone(), self.on_change.clone());
            move |i, window, cx| {
                if on == Some(i) {
                    return;
                }
                let id = ids[i].clone();
                if !controlled {
                    state.update(cx, |s, cx| {
                        s.selected = Some(id.clone());
                        cx.notify();
                    });
                }
                if let Some(f) = &on_change {
                    f(&id, window, cx);
                }
            }
        });
        let manual = self.manual;
        // Moves the keyboard to the tab `to` gives from the focused one, selecting it unless
        // manual.
        let mover = |to: fn(&[bool], usize) -> Option<usize>| {
            let (handles, enabled, select, scroll) = (
                handles.clone(),
                enabled.clone(),
                select.clone(),
                scroll.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                let from = handles
                    .iter()
                    .position(|h| h.is_focused(window))
                    .or(stop)
                    .unwrap_or(0);
                if let Some(i) = to(&enabled, from) {
                    window.focus(&handles[i], cx);
                    scroll.scroll_to_item(i);
                    if !manual {
                        select(i, window, cx);
                    }
                    window.refresh();
                }
            }
        };
        let prev = mover(|e, at| step(e, at, -1));
        let next = mover(|e, at| step(e, at, 1));
        let first = mover(|e, _| e.iter().position(|x| *x));
        let last = mover(|e, _| e.iter().rposition(|x| *x));

        let look = Look {
            variant: self.variant,
            overflow: self.overflow,
            touch: self.touch,
        };
        let mut tabs = self.tabs;
        let triggers: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let select = select.clone();
                trigger(
                    &self.id,
                    tab,
                    on == Some(i),
                    stop == Some(i),
                    &handles[i],
                    look,
                    &theme,
                )
                .on_click(move |_, window, cx| select(i, window, cx))
            })
            .collect();
        let panel = on
            .filter(|i| !tabs[*i].children.is_empty())
            .map(|i| panel(&self.id, tabs.swap_remove(i)));

        let list = list(&self.id, self.label, look, &theme)
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &First, w, cx| first(w, cx))
            .on_action(move |_: &Last, w, cx| last(w, cx))
            .when(self.stretch, |d| d.w_full())
            .when(self.overflow == TabsOverflow::Scroll, |d| {
                d.overflow_x_scroll().track_scroll(&scroll)
            })
            .children(triggers);

        let mut root = div()
            .flex()
            .flex_col()
            .items_start()
            .gap(css(8.))
            .min_w_0()
            .child(list)
            .children(panel);
        gpui::Refineable::refine(root.style(), &self.style);
        root
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, accesskit};

    use super::*;

    fn node(el: &impl gpui::Element) -> (Option<Role>, accesskit::Node) {
        let mut node = accesskit::Node::new(Role::Unknown);
        el.write_a11y_info(&mut node);
        (el.a11y_role(), node)
    }

    #[gpui::test]
    fn the_list_tabs_and_panel_have_their_roles(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let theme = cx.theme().clone();
            let look = Look {
                variant: TabsVariant::Default,
                overflow: TabsOverflow::Shrink,
                touch: false,
            };
            let id = ElementId::from("t");
            let l = list(&id, Some("Settings".into()), look, &theme);
            let (role, n) = node(&l);
            assert_eq!(role, Some(Role::TabList));
            assert_eq!(n.label(), Some("Settings"));

            let focus = cx.focus_handle();
            let tab = Tab::new("a", "Account");
            let t = trigger(&id, &tab, true, true, &focus, look, &theme)
                .render(window, cx)
                .into_element();
            let (role, n) = node(&t);
            assert_eq!(role, Some(Role::Tab));
            assert_eq!(n.is_selected(), Some(true));
            assert_eq!(n.label(), Some("Account"));
            let off = Tab::new("b", "Billing").disabled(true);
            let t = trigger(&id, &off, false, false, &focus, look, &theme)
                .render(window, cx)
                .into_element();
            let (_, n) = node(&t);
            assert_eq!(n.is_selected(), Some(false));
            assert!(n.is_disabled());

            let p = panel(&id, Tab::new("a", "Account").child("Hi"));
            let (role, n) = node(&p);
            assert_eq!(role, Some(Role::TabPanel));
            assert_eq!(n.label(), Some("Account"));
        });
    }
}
