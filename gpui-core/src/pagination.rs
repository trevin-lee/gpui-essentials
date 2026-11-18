//! Pagination: Previous, the page numbers around the current page with ellipses for the rest,
//! and Next. Each is a link in the Tab order, pressed with a click, a tap, Enter or Space; the
//! current page says so to assistive tech. When it doesn't fit its width it drops Previous's and
//! Next's words, then shows only "Page 3 of 10" between the two arrows.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Pagination;
//! # struct Results { page: usize }
//! # impl Render for Results {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Pagination::new("pages")
//!         .total(20)
//!         .page(self.page)
//!         .on_page(cx.listener(|this, page: &usize, _, cx| {
//!             this.page = *page;
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, accesskit::AriaCurrent, canvas, div, prelude::FluentBuilder as _, px, rems,
};

use crate::base::{ActiveTheme as _, Current, Disableable as _, Icon, IconName, Press, Theme};

type OnPage = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// One place in the row of page numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    Page(usize),
    /// Pages left out.
    Gap,
}

/// The pages shown out of `total` around `page` (1-based): the first and the last, `siblings`
/// either side of the current one, and an ellipsis for each run left out (a run of one is shown
/// instead). The row keeps one length as the current page moves, so the links don't jump.
pub(crate) fn slots(total: usize, page: usize, siblings: usize) -> Vec<Slot> {
    let s = siblings;
    if total == 0 {
        return Vec::new();
    }
    let page = page.clamp(1, total);
    if total <= 2 * s + 5 {
        return (1..=total).map(Slot::Page).collect();
    }
    let start = page.saturating_sub(s).min(total - 2 * s - 2).max(3);
    let end = (page + s).max(2 * s + 3).min(total - 2);
    let mut out = vec![Slot::Page(1)];
    out.push(if start > 3 { Slot::Gap } else { Slot::Page(2) });
    out.extend((start..=end).map(Slot::Page));
    out.push(if end < total - 2 {
        Slot::Gap
    } else {
        Slot::Page(total - 1)
    });
    out.push(Slot::Page(total));
    out
}

/// How much of it fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// Previous and Next with their words.
    Full,
    /// Previous and Next as arrows.
    Arrows,
    /// The arrows and "Page 3 of 10".
    Compact,
}

/// Previous, page numbers and Next: a navigation landmark of links.
#[derive(IntoElement)]
pub struct Pagination {
    id: ElementId,
    total: usize,
    page: usize,
    siblings: usize,
    on_page: Option<OnPage>,
    compact: bool,
    label: SharedString,
    touch: bool,
    style: StyleRefinement,
}

impl Pagination {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            total: 1,
            page: 1,
            siblings: 1,
            on_page: None,
            compact: false,
            label: "Pagination".into(),
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// How many pages there are.
    pub fn total(mut self, total: usize) -> Self {
        self.total = total;
        self
    }

    /// The current page, counting from 1.
    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// How many pages to show either side of the current one (1 unless given).
    pub fn siblings(mut self, siblings: usize) -> Self {
        self.siblings = siblings;
        self
    }

    /// Told the page a link asks for. The host makes it the current one.
    pub fn on_page(mut self, f: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_page = Some(Rc::new(f));
        self
    }

    /// Always the compact form ("Page 3 of 10" between the arrows), not only when narrow.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// What it is to assistive tech ("Pagination" unless given).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Links 44 points each way, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for Pagination {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// What a pagination remembers: its width when last drawn.
#[derive(Default)]
struct PaginationState {
    width: Option<Pixels>,
}

/// A link's colours: a ghost button's, or an outlined one's for the current page.
struct Paint {
    fill: Hsla,
    border: Hsla,
    hover: Hsla,
}

fn paint(current: bool, theme: &Theme) -> Paint {
    let dark = theme.mode.is_dark();
    let none = gpui::transparent_black();
    match (current, dark) {
        (true, true) => Paint {
            fill: theme.control.opacity(0.3),
            border: theme.input,
            hover: theme.control.opacity(0.5),
        },
        (true, false) => Paint {
            fill: theme.background,
            border: theme.input,
            hover: theme.muted,
        },
        (false, _) => Paint {
            fill: none,
            border: none,
            hover: if dark {
                theme.muted.opacity(0.5)
            } else {
                theme.muted
            },
        },
    }
}

/// A link: a page's, or Previous or Next.
fn link(
    nav: &ElementId,
    key: &str,
    label: SharedString,
    current: bool,
    touch: bool,
    theme: &Theme,
) -> Press {
    let p = paint(current, theme);
    let side = if touch {
        theme.touch_target.into()
    } else {
        gpui::DefiniteLength::from(css(36.))
    };
    let selector = format!("page-{key}");
    Press::new(ElementId::NamedChild(
        std::sync::Arc::new(nav.clone()),
        key.to_string().into(),
    ))
    .role(Role::Link)
    .accessibility_label(label)
    .debug_selector(move || selector.clone())
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .gap(css(4.))
    .h(side)
    .min_w(side)
    .rounded(theme.radius_md())
    .border_1()
    .border_color(p.border)
    .bg(p.fill)
    .text_color(theme.foreground)
    .text_size(theme.text.sm)
    .font_weight(gpui::FontWeight::MEDIUM)
    .whitespace_nowrap()
    .hover({
        let h = p.hover;
        move |s| s.bg(h)
    })
}

/// Page `n`'s link.
fn page_link(nav: &ElementId, n: usize, current: bool, touch: bool, theme: &Theme) -> Press {
    link(
        nav,
        &n.to_string(),
        format!("Page {n}").into(),
        current,
        touch,
        theme,
    )
    .px(css(10.))
    .child(n.to_string())
}

/// A page's link, the current page to assistive tech when it's the current one.
fn current_page(
    press: Press,
    current: bool,
    window: &mut Window,
    cx: &mut App,
) -> Current<impl gpui::Element> {
    let el = press.render(window, cx).into_element();
    Current::new(el, current.then_some(AriaCurrent::Page))
}

impl RenderOnce for Pagination {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| PaginationState::default());
        let total = self.total.max(1);
        let page = self.page.clamp(1, total);
        let row = slots(total, page, self.siblings);

        // The widths of the forms, as drawn, to pick the one that fits.
        let rem = window.rem_size();
        let font_size = theme.text.sm.to_pixels(rem);
        let text_width = |text: &str, window: &mut Window| {
            let mut style = window.text_style();
            style.font_size = font_size.into();
            style.font_weight = gpui::FontWeight::MEDIUM;
            let run = style.to_run(text.len());
            window
                .text_system()
                .shape_line(
                    SharedString::from(text.to_string()),
                    font_size,
                    &[run],
                    None,
                )
                .width
        };
        let side = if self.touch {
            theme.touch_target
        } else {
            css(36.).to_pixels(rem)
        };
        let gap = css(4.).to_pixels(rem);
        let pad = css(10.).to_pixels(rem);
        let slot_w = |s: &Slot, window: &mut Window| match s {
            Slot::Gap => side,
            Slot::Page(n) => side.max(text_width(&n.to_string(), window) + pad * 2.),
        };
        let pages_w: Pixels =
            row.iter().map(|s| slot_w(s, window)).sum::<Pixels>() + gap * row.len() as f32;
        let worded = |w: &str, window: &mut Window| {
            text_width(w, window) + font_size + gap + pad * 2. + px(2.)
        };
        let full = pages_w + worded("Previous", window) + worded("Next", window) + gap;
        let arrows = pages_w + side * 2. + gap;
        let avail = state.read(cx).width;
        let fits = |w: Pixels| avail.is_none_or(|a| w <= a);
        let form = if self.compact {
            Form::Compact
        } else if fits(full) {
            Form::Full
        } else if fits(arrows) {
            Form::Arrows
        } else {
            Form::Compact
        };

        let on_page = self.on_page.clone();
        let go = move |to: usize| {
            let on_page = on_page.clone();
            move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                if let Some(f) = &on_page {
                    f(&to, window, cx);
                }
            }
        };
        let touch = self.touch;
        let step = |key: &str, label: &str, icon: IconName, to: Option<usize>, before: bool| {
            let words = form == Form::Full;
            link(
                &self.id,
                key,
                label.to_string().into(),
                false,
                touch,
                &theme,
            )
            .px(if words { css(10.) } else { css(0.) })
            .disabled(to.is_none())
            .when(to.is_none(), |p| p.opacity(0.5))
            .when_some(to, |p, to| p.on_click(go(to)))
            .when(before, |p| p.child(Icon::new(icon)))
            .when(words, |p| p.child(if before { "Previous" } else { "Next" }))
            .when(!before, |p| p.child(Icon::new(icon)))
        };
        let prev = step(
            "prev",
            "Go to the previous page",
            IconName::ChevronLeft,
            (page > 1).then(|| page - 1),
            true,
        );
        let next = step(
            "next",
            "Go to the next page",
            IconName::ChevronRight,
            (page < total).then(|| page + 1),
            false,
        );

        let middle: Vec<gpui::AnyElement> = if form == Form::Compact {
            vec![
                div()
                    .debug_selector(|| "page-compact".into())
                    .min_w_0()
                    .px(css(8.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Page {page} of {total}"))
                    .into_any_element(),
            ]
        } else {
            let mut gaps = 0;
            row.iter()
                .map(|s| match *s {
                    Slot::Gap => {
                        gaps += 1;
                        let selector = format!("page-gap-{}", gaps - 1);
                        div()
                            .debug_selector(move || selector.clone())
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(side)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::Ellipsis))
                            .into_any_element()
                    }
                    Slot::Page(n) => {
                        let current = n == page;
                        let press = page_link(&self.id, n, current, touch, &theme)
                            .when(!current, |p| p.on_click(go(n)));
                        current_page(press, current, window, cx).into_any_element()
                    }
                })
                .collect()
        };

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

        let mut nav = div()
            .id(self.id.clone())
            .role(Role::Navigation)
            .aria_label(self.label.clone())
            .debug_selector(|| "pagination".into())
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .min_w_0()
            .gap(css(4.))
            .child(measure)
            .child(prev)
            .children(middle)
            .child(next);
        gpui::Refineable::refine(nav.style(), &self.style);
        nav
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::Slot::{Gap, Page as P};
    use super::*;

    #[test]
    fn the_row_keeps_its_length_round_the_current_page() {
        assert_eq!(slots(5, 3, 1), [P(1), P(2), P(3), P(4), P(5)]);
        assert_eq!(slots(10, 1, 1), [P(1), P(2), P(3), P(4), P(5), Gap, P(10)]);
        assert_eq!(slots(10, 4, 1), [P(1), P(2), P(3), P(4), P(5), Gap, P(10)]);
        assert_eq!(slots(10, 5, 1), [P(1), Gap, P(4), P(5), P(6), Gap, P(10)]);
        assert_eq!(slots(10, 7, 1), [P(1), Gap, P(6), P(7), P(8), P(9), P(10)]);
        assert_eq!(slots(10, 10, 1), [P(1), Gap, P(6), P(7), P(8), P(9), P(10)]);
        assert_eq!(slots(10, 99, 1), slots(10, 10, 1), "past the end: the last");
        assert_eq!(slots(20, 10, 2).len(), 9);
        assert!(slots(0, 1, 1).is_empty());
    }

    #[gpui::test]
    fn the_current_page_says_so(cx: &mut TestAppContext) {
        cx.update(crate::base::init);
        let (_, vcx) = cx.add_window_view(|_, _| gpui::Empty);
        vcx.update(|window, cx| {
            let theme = cx.theme().clone();
            let id = ElementId::from("p");
            // With a focus handle of its own, as outside a draw it can't keep one.
            let focus = cx.focus_handle();
            for (n, current) in [(3, true), (4, false)] {
                let press = page_link(&id, n, current, false, &theme).track_focus(&focus);
                let el = current_page(press, current, window, cx);
                let mut node = accesskit::Node::new(Role::Unknown);
                el.write_a11y_info(&mut node);
                assert_eq!(el.a11y_role(), Some(Role::Link));
                assert_eq!(node.label(), Some(format!("Page {n}").as_str()));
                assert_eq!(node.aria_current(), current.then_some(AriaCurrent::Page));
            }
        });
    }
}
