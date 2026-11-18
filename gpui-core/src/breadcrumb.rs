//! A breadcrumb: the path to the page you're on, each step a link back to it, separated by
//! chevrons; the last step is the current page, shown but not a link. When the path doesn't fit
//! its width (or is longer than [`Breadcrumb::max_items`]), its middle folds into an ellipsis
//! that opens a menu of the steps it hides.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Breadcrumb, BreadcrumbItem};
//! # struct Page;
//! # impl Render for Page {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Breadcrumb::new("path")
//!         .item(BreadcrumbItem::new("home", "Home"))
//!         .item(BreadcrumbItem::new("docs", "Docs"))
//!         .item(BreadcrumbItem::new("breadcrumb", "Breadcrumb"))
//!         .on_navigate(cx.listener(|_, id: &SharedString, _, _| {
//!             // Go to `id`.
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    Role, SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    accesskit::AriaCurrent, canvas, div, prelude::FluentBuilder as _, px, rems,
};

use crate::base::{
    ActiveTheme as _, Current, Icon, IconName, Popover, PopupMenu, PopupMenuItem, Press, Theme,
};

type OnNavigate = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// One step of the path.
pub struct BreadcrumbItem {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
}

impl BreadcrumbItem {
    /// `id` is what [`Breadcrumb::on_navigate`] is told when it's pressed.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }

    /// An icon before the label (a house for Home).
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// The path to the current page, its last item. Each other item is a link: a press, or Enter
/// or Space on it, tells [`on_navigate`](Self::on_navigate). It fills the width it's given.
#[derive(IntoElement)]
pub struct Breadcrumb {
    id: ElementId,
    items: Vec<BreadcrumbItem>,
    on_navigate: Option<OnNavigate>,
    max_items: Option<usize>,
    label: SharedString,
    touch: bool,
    style: StyleRefinement,
}

impl Breadcrumb {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            on_navigate: None,
            max_items: None,
            label: "Breadcrumb".into(),
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn item(mut self, item: BreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = BreadcrumbItem>) -> Self {
        self.items.extend(items);
        self
    }

    /// Told the item pressed, or picked from the ellipsis's menu.
    pub fn on_navigate(
        mut self,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(f));
        self
    }

    /// The most items shown, the first and the last among them (at least two): past it, the
    /// middle folds into the ellipsis even when there's room.
    pub fn max_items(mut self, max: usize) -> Self {
        self.max_items = Some(max.max(2));
        self
    }

    /// What it is to assistive tech ("Breadcrumb" unless given).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Links and the ellipsis 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for Breadcrumb {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// What a breadcrumb remembers between frames.
#[derive(Default)]
struct BreadcrumbState {
    /// Its width when last drawn.
    width: Option<Pixels>,
    /// Whether the ellipsis's menu is open.
    open: bool,
}

/// How many of the last items to show after the first and the ellipsis, or `None` to show
/// them all: all if they fit `avail` (when it's known) and number no more than `max`; else as
/// many of the last as fit, at least one.
pub(crate) fn plan(
    widths: &[Pixels],
    separator: Pixels,
    ellipsis: Pixels,
    avail: Option<Pixels>,
    max: Option<usize>,
) -> Option<usize> {
    let n = widths.len();
    if n <= 2 {
        return None;
    }
    let fits = |w: Pixels| avail.is_none_or(|a| w <= a);
    let all: Pixels = widths.iter().copied().sum::<Pixels>() + separator * (n - 1) as f32;
    let most = max.map_or(n - 2, |m| (m - 1).clamp(1, n - 2));
    if fits(all) && max.is_none_or(|m| n <= m) {
        return None;
    }
    (1..=most)
        .rev()
        .find(|&k| {
            let shown: Pixels = widths[0] + widths[n - k..].iter().copied().sum::<Pixels>();
            fits(shown + ellipsis + separator * (k + 1) as f32)
        })
        .or(Some(1))
}

/// A link to an item: a Link to assistive tech.
fn link(list: &ElementId, item: &BreadcrumbItem, touch: bool, theme: &Theme) -> Press {
    let id = item.id.clone();
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(list.clone()),
        item.id.clone(),
    ))
    .role(Role::Link)
    .accessibility_label(item.label.clone())
    .debug_selector(move || format!("crumb-{id}"))
    .flex()
    .min_w_0()
    .items_center()
    .gap(css(6.))
    .rounded(theme.radius_sm())
    .when(touch, |p| p.min_h(theme.touch_target))
    .hover({
        let c = theme.foreground;
        move |s| s.text_color(c)
    })
    .when_some(item.icon, |p, i| p.child(Icon::new(i)))
    .child(label(item.label.clone()))
}

fn label(text: SharedString) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .child(text)
}

/// The current page: a link to assistive tech that's the current page and can't be followed.
fn page(list: &ElementId, item: &BreadcrumbItem, theme: &Theme) -> Current<Stateful<gpui::Div>> {
    let id = item.id.clone();
    let el = div()
        .id(ElementId::NamedChild(
            std::sync::Arc::new(list.clone()),
            item.id.clone(),
        ))
        .role(Role::Link)
        .aria_label(item.label.clone())
        .debug_selector(move || format!("crumb-{id}"))
        .flex()
        .min_w_0()
        .items_center()
        .gap(css(6.))
        .text_color(theme.foreground)
        .when_some(item.icon, |p, i| p.child(Icon::new(i)))
        .child(label(item.label.clone()));
    Current::new(el, Some(AriaCurrent::Page))
}

fn separator() -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .child(Icon::new(IconName::ChevronRight).size(css(14.)))
}

impl RenderOnce for Breadcrumb {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| BreadcrumbState::default());
        let (avail, open) = {
            let s = state.read(cx);
            (s.width, s.open)
        };
        let rem = window.rem_size();
        let font_size = theme.text.sm.to_pixels(rem);
        let gap = css(6.).to_pixels(rem);
        let separator_w = css(14.).to_pixels(rem) + gap * 2.;
        let ellipsis_w = if self.touch {
            theme.touch_target
        } else {
            css(24.).to_pixels(rem)
        };
        // Each item's width on one line, as drawn.
        let widths: Vec<Pixels> = {
            let mut style = window.text_style();
            style.font_size = font_size.into();
            self.items
                .iter()
                .map(|item| {
                    let run = style.to_run(item.label.len());
                    let text = window
                        .text_system()
                        .shape_line(item.label.clone(), font_size, &[run], None)
                        .width;
                    text + if item.icon.is_some() {
                        font_size + gap
                    } else {
                        px(0.)
                    }
                })
                .collect()
        };
        let trailing = plan(&widths, separator_w, ellipsis_w, avail, self.max_items);

        let n = self.items.len();
        let navigate = self.on_navigate.clone();
        let mut row: Vec<gpui::AnyElement> = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            if trailing.is_some_and(|k| i >= 1 && i < n - k) {
                continue;
            }
            if i > 0 {
                row.push(separator().into_any_element());
            }
            if i == n - 1 {
                row.push(page(&self.id, item, &theme).into_any_element());
            } else {
                let navigate = navigate.clone();
                let id = item.id.clone();
                row.push(
                    link(&self.id, item, self.touch, &theme)
                        .on_click(move |_, window, cx| {
                            if let Some(f) = &navigate {
                                f(&id, window, cx);
                            }
                        })
                        .into_any_element(),
                );
            }
            if i == 0 && trailing.is_some() {
                row.push(separator().into_any_element());
                row.push(self.ellipsis(&state, open, n - trailing.unwrap_or(1), &theme));
            }
        }

        // Measures the width it's given; when that changes, it's drawn again to fit.
        let measure = canvas(
            {
                let state = state.clone();
                move |bounds, window, cx| {
                    let width = bounds.size.width;
                    if state.read(cx).width != Some(width) {
                        state.update(cx, |s, _| s.width = Some(width));
                        window.defer(cx, move |_, cx| state.update(cx, |_, cx| cx.notify()));
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let mut root = div()
            .id(self.id.clone())
            .role(Role::Navigation)
            .aria_label(self.label.clone())
            .debug_selector(|| "breadcrumb".into())
            .relative()
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .gap(css(6.))
            .text_size(theme.text.sm)
            .text_color(theme.muted_foreground)
            .when(self.touch, |d| d.min_h(theme.touch_target))
            .child(measure)
            .children(row);
        gpui::Refineable::refine(root.style(), &self.style);
        root
    }
}

impl Breadcrumb {
    /// The ellipsis standing for the folded items, opening a menu of them.
    /// `end`: the first item shown after it.
    fn ellipsis(
        &self,
        state: &gpui::Entity<BreadcrumbState>,
        open: bool,
        end: usize,
        theme: &Theme,
    ) -> gpui::AnyElement {
        let folded: Vec<PopupMenuItem> = self.items[1..end]
            .iter()
            .map(|i| {
                let mut item = PopupMenuItem::new(i.id.clone(), i.label.clone());
                if let Some(icon) = i.icon {
                    item = item.icon(icon);
                }
                item
            })
            .collect();
        let count = folded.len();
        let set_open = {
            let state = state.clone();
            move |to: bool, cx: &mut App| {
                state.update(cx, |s, cx| {
                    if s.open != to {
                        s.open = to;
                        cx.notify();
                    }
                })
            }
        };
        let toggle = set_open.clone();
        let dismiss = set_open.clone();
        let navigate = self.on_navigate.clone();
        let side = if self.touch {
            theme.touch_target.into()
        } else {
            gpui::DefiniteLength::from(css(24.))
        };
        let trigger = Press::new(ElementId::NamedChild(
            std::sync::Arc::new(self.id.clone()),
            "more".into(),
        ))
        .accessibility_label(format!("Show {count} more"))
        .aria_expanded(open)
        .debug_selector(|| "crumb-more".into())
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(side)
        .rounded(theme.radius_sm())
        .hover({
            let c = theme.foreground;
            move |s| s.text_color(c)
        })
        .child(Icon::new(IconName::Ellipsis))
        .on_click(move |_, _, cx| toggle(!open, cx));
        Popover::new(ElementId::NamedChild(
            std::sync::Arc::new(self.id.clone()),
            "more-menu".into(),
        ))
        .open(open)
        .match_trigger_width(false)
        .on_dismiss(move |_, _, cx| dismiss(false, cx))
        .trigger(trigger)
        .p(css(4.))
        .child(
            PopupMenu::new(ElementId::NamedChild(
                std::sync::Arc::new(self.id.clone()),
                "more-items".into(),
            ))
            .touch(self.touch)
            .items(folded)
            .on_select(move |pick, window, cx| {
                set_open(false, cx);
                if let Some(f) = &navigate {
                    f(&pick.id, window, cx);
                }
            }),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    #[test]
    fn it_folds_the_middle_to_fit() {
        let w = |v: &[f32]| v.iter().map(|x| px(*x)).collect::<Vec<_>>();
        let items = w(&[50., 60., 70., 80., 40.]);
        let (sep, ell) = (px(10.), px(20.));
        // All: 300 and four separators.
        assert_eq!(plan(&items, sep, ell, Some(px(340.)), None), None);
        assert_eq!(
            plan(&items, sep, ell, None, None),
            None,
            "width not known yet"
        );
        // First, ellipsis, and the last three: 50 + 20 + 190 + 40 of separators.
        assert_eq!(plan(&items, sep, ell, Some(px(339.)), None), Some(3));
        assert_eq!(plan(&items, sep, ell, Some(px(299.)), None), Some(2));
        assert_eq!(
            plan(&items, sep, ell, Some(px(10.)), None),
            Some(1),
            "at least the last"
        );
        assert_eq!(
            plan(&items, sep, ell, None, Some(3)),
            Some(2),
            "past the most"
        );
        assert_eq!(plan(&items, sep, ell, None, Some(5)), None);
        assert_eq!(
            plan(&items[..2], sep, ell, Some(px(1.)), Some(2)),
            None,
            "two never fold"
        );
    }

    #[gpui::test]
    fn links_and_the_page_have_their_roles(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let theme = cx.theme().clone();
            let id = ElementId::from("b");
            let item = BreadcrumbItem::new("docs", "Docs");
            // With a focus handle of its own, as outside a draw it can't keep one.
            let focus = cx.focus_handle();
            let l = link(&id, &item, false, &theme)
                .track_focus(&focus)
                .render(window, cx)
                .into_element();
            let mut node = accesskit::Node::new(Role::Unknown);
            l.write_a11y_info(&mut node);
            assert_eq!(l.a11y_role(), Some(Role::Link));
            assert_eq!(node.label(), Some("Docs"));
            assert_eq!(node.aria_current(), None);

            let p = page(&id, &item, &theme);
            let mut node = accesskit::Node::new(Role::Unknown);
            p.write_a11y_info(&mut node);
            assert_eq!(p.a11y_role(), Some(Role::Link));
            assert_eq!(node.aria_current(), Some(AriaCurrent::Page));
        });
    }
}
