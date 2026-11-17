//! An accordion, as shadcn/ui draws one: items stacked with a rule between them, each a header
//! that shows or hides its content. One item opens at a time (opening another closes it), or
//! several with [`multiple`](Accordion::multiple).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Accordion, AccordionItem};
//! # struct Faq;
//! # impl Render for Faq {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     Accordion::new("faq")
//!         .default_open(["shipping"])
//!         .item(AccordionItem::new("shipping").title("Shipping").child("Three to five days."))
//!         .item(AccordionItem::new("returns").title("Returns").child("Within thirty days."))
//! }
//! # }
//! ```
//!
//! Every header is a Tab stop and a [`Press`]: Enter, Space, a click or a tap opens or closes
//! it. Up and Down move between the headers (round from the last to the first), Home and End go
//! to the ends, passing over disabled ones. Assistive technology hears whether each is expanded.
//! The chevron points down when closed and up when open; nothing animates. The host can keep
//! which items are open ([`open`](Accordion::open) and [`on_change`](Accordion::on_change)) or
//! leave it to the accordion, which remembers it under its id.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement, Refineable as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::press::Press;
use crate::size::Disableable;
use crate::theme::ActiveTheme as _;

actions!(
    gpui_core_accordion,
    [
        /// The header above the one with the keyboard (the last from the first).
        Previous,
        /// The header below it (the first from the last).
        Next,
        First,
        Last
    ]
);

const CONTEXT: &str = "GpuiCoreAccordion";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("down", Next, c),
        KeyBinding::new("home", First, c),
        KeyBinding::new("end", Last, c),
    ]);
}

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

type OnChange = Rc<dyn Fn(&Vec<SharedString>, &mut Window, &mut App)>;

/// One of an accordion's items: a header with its title, and content (its children).
pub struct AccordionItem {
    id: SharedString,
    title: SharedString,
    disabled: bool,
    content: Vec<AnyElement>,
}

impl AccordionItem {
    /// `id` names it in [`Accordion::open`] and [`Accordion::on_change`].
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: SharedString::default(),
            disabled: false,
            content: Vec::new(),
        }
    }

    /// The header's text. It wraps.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }
}

impl Disableable for AccordionItem {
    /// Its header ignores presses, takes no focus and is passed over by the arrows.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ParentElement for AccordionItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements)
    }
}

/// Items that open and close. See the module's documentation for how it behaves.
#[derive(IntoElement)]
pub struct Accordion {
    id: ElementId,
    items: Vec<AccordionItem>,
    multiple: bool,
    open: Option<Vec<SharedString>>,
    default_open: Vec<SharedString>,
    on_change: Option<OnChange>,
    touch: bool,
    style: StyleRefinement,
}

impl Accordion {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            multiple: false,
            open: None,
            default_open: Vec::new(),
            on_change: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn item(mut self, item: AccordionItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = AccordionItem>) -> Self {
        self.items.extend(items);
        self
    }

    /// Whether several items can be open at once (otherwise opening one closes the other).
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// The items that are open, for a host that keeps them: they change only when the host
    /// passes new ones (after [`on_change`](Self::on_change)).
    pub fn open<S: Into<SharedString>>(mut self, ids: impl IntoIterator<Item = S>) -> Self {
        self.open = Some(ids.into_iter().map(Into::into).collect());
        self
    }

    /// The items open at first, when the accordion keeps them itself (no
    /// [`open`](Self::open)).
    pub fn default_open<S: Into<SharedString>>(mut self, ids: impl IntoIterator<Item = S>) -> Self {
        self.default_open = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Told the items that should be open after a header was pressed, in the accordion's
    /// order. A host that keeps them passes them back with [`open`](Self::open); otherwise
    /// it's for information, and the accordion has already changed.
    pub fn on_change(
        mut self,
        f: impl Fn(&Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Headers at least 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for Accordion {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What the accordion adds to a header: whether it's expanded, and disabled.
fn wire(header: Press, open: bool, disabled: bool) -> Press {
    header.aria_expanded(open).disabled(disabled)
}

/// The items open after pressing `id`'s header.
fn toggled(
    open: &[SharedString],
    id: &SharedString,
    multiple: bool,
    order: &[SharedString],
) -> Vec<SharedString> {
    let was = open.contains(id);
    order
        .iter()
        .filter(|o| {
            if *o == id {
                !was
            } else {
                multiple && open.contains(o)
            }
        })
        .cloned()
        .collect()
}

/// The next header from `from` that can take the focus, `step` at a time, going round.
fn step(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
    let n = enabled.len() as isize;
    (1..=n)
        .map(|k| (from as isize + step * k).rem_euclid(n) as usize)
        .find(|&i| enabled[i])
}

impl RenderOnce for Accordion {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let theme = cx.theme().clone();
        let n = self.items.len();
        let handles = window.use_keyed_state(
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "focus".into()),
            cx,
            |_, _| Vec::<FocusHandle>::new(),
        );
        handles.update(cx, |h, cx| {
            while h.len() < n {
                h.push(cx.focus_handle());
            }
            h.truncate(n);
        });
        let handles: Rc<Vec<FocusHandle>> = Rc::new(handles.read(cx).clone());
        // Which items are open, when the accordion keeps them.
        let kept = self.open.is_none().then(|| {
            let default = self.default_open.clone();
            window.use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "open".into()),
                cx,
                move |_, _| default,
            )
        });
        let open: Rc<Vec<SharedString>> = Rc::new(match (&self.open, &kept) {
            (Some(open), _) => open.clone(),
            (None, Some(kept)) => kept.read(cx).clone(),
            (None, None) => Vec::new(),
        });
        let order: Rc<Vec<SharedString>> =
            Rc::new(self.items.iter().map(|i| i.id.clone()).collect());
        let enabled: Rc<Vec<bool>> = Rc::new(self.items.iter().map(|i| !i.disabled).collect());

        // Moves the keyboard to the header `to` gives from the one that has it.
        let mover = |to: fn(&[bool], usize) -> Option<usize>| {
            let (handles, enabled) = (handles.clone(), enabled.clone());
            move |window: &mut Window, cx: &mut App| {
                let Some(at) = handles.iter().position(|h| h.is_focused(window)) else {
                    return;
                };
                if let Some(i) = to(&enabled, at) {
                    window.focus(&handles[i], cx);
                }
            }
        };
        let prev = mover(|e, at| step(e, at, -1));
        let next = mover(|e, at| step(e, at, 1));
        let first = mover(|e, _| e.iter().position(|x| *x));
        let last = mover(|e, _| e.iter().rposition(|x| *x));

        let multiple = self.multiple;
        let touch = self.touch;
        let items = self.items.into_iter().enumerate().map(|(i, item)| {
            let is_open = open.contains(&item.id);
            let last_item = i + 1 == n;
            let press = {
                let (open, order, kept, on_change, id) = (
                    open.clone(),
                    order.clone(),
                    kept.clone(),
                    self.on_change.clone(),
                    item.id.clone(),
                );
                move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                    let now = toggled(&open, &id, multiple, &order);
                    if let Some(kept) = &kept {
                        kept.update(cx, |o, cx| {
                            *o = now.clone();
                            cx.notify();
                        });
                    }
                    if let Some(f) = &on_change {
                        f(&now, window, cx);
                    }
                }
            };
            let item_id = item.id.clone();
            let chevron_id = item.id.clone();
            let chevron = if is_open {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            };
            let header = Press::new(ElementId::NamedChild(
                std::sync::Arc::new(ElementId::Name("accordion-header".into())),
                item.id.clone(),
            ))
            .track_focus(&handles[i])
            .debug_selector(move || format!("accordion-header-{item_id}"))
            .flex()
            .items_start()
            .justify_between()
            .gap(css(16.))
            .w_full()
            .min_w_0()
            .py(css(16.))
            .when(touch, |p| p.min_h(theme.touch_target))
            .rounded(theme.radius_md())
            .text_size(theme.text.sm)
            .font_weight(FontWeight::MEDIUM)
            .when(!item.disabled, |p| p.hover(|s| s.underline()))
            .when(item.disabled, |p| p.opacity(0.5))
            .child(div().flex_1().min_w_0().child(item.title.clone()))
            .child(
                div()
                    .debug_selector(move || {
                        format!(
                            "accordion-chevron-{chevron_id}-{}",
                            if chevron == IconName::ChevronUp {
                                "up"
                            } else {
                                "down"
                            }
                        )
                    })
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(rems(theme.text.sm.0 * 1.5))
                    .child(
                        Icon::new(chevron)
                            .size(css(16.))
                            .text_color(theme.muted_foreground),
                    ),
            )
            .on_click(press);
            let content_id = item.id.clone();
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .when(!last_item, |d| d.border_b_1().border_color(theme.border))
                .child(wire(header, is_open, item.disabled))
                .when(is_open, |d| {
                    d.child(
                        div()
                            .id(ElementId::NamedChild(
                                std::sync::Arc::new(ElementId::Name("accordion-content".into())),
                                item.id.clone(),
                            ))
                            .role(Role::Region)
                            .aria_label(item.title)
                            .debug_selector(move || format!("accordion-content-{content_id}"))
                            .min_w_0()
                            .pb(css(16.))
                            .text_size(theme.text.sm)
                            .children(item.content),
                    )
                })
        });
        let mut el = div()
            .id(self.id)
            .key_context(CONTEXT)
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &First, w, cx| first(w, cx))
            .on_action(move |_: &Last, w, cx| last(w, cx))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .children(items);
        el.style().refine(&self.style);
        el
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, TestAppContext, accesskit};

    use super::*;

    fn ids(v: &[&str]) -> Vec<SharedString> {
        v.iter()
            .map(|s| SharedString::from(s.to_string()))
            .collect()
    }

    #[test]
    fn one_open_at_a_time_unless_several_may_be() {
        let order = ids(&["a", "b", "c"]);
        let a: SharedString = "a".into();
        let b: SharedString = "b".into();
        assert_eq!(toggled(&ids(&["a"]), &b, false, &order), ids(&["b"]));
        assert_eq!(toggled(&ids(&["a"]), &a, false, &order), ids(&[]));
        assert_eq!(toggled(&ids(&["c"]), &a, true, &order), ids(&["a", "c"]));
        assert_eq!(toggled(&ids(&["a", "c"]), &a, true, &order), ids(&["c"]));
    }

    #[test]
    fn the_arrows_go_round_and_pass_over_disabled_headers() {
        let e = [true, false, true, true];
        assert_eq!(step(&e, 0, 1), Some(2));
        assert_eq!(step(&e, 3, 1), Some(0));
        assert_eq!(step(&e, 0, -1), Some(3));
        assert_eq!(step(&e, 2, -1), Some(0));
        assert_eq!(step(&[false, true], 1, 1), Some(1), "the only one");
    }

    #[gpui::test]
    fn a_header_says_whether_it_is_expanded(cx: &mut TestAppContext) {
        let vcx = cx.add_empty_window();
        let node = |open: bool, disabled: bool, vcx: &mut gpui::VisualTestContext| {
            vcx.update(|window, cx| {
                let el = wire(
                    Press::new("h").track_focus(&cx.focus_handle()),
                    open,
                    disabled,
                )
                .render(window, cx)
                .into_element();
                let mut node = accesskit::Node::new(el.a11y_role().unwrap());
                el.write_a11y_info(&mut node);
                node
            })
        };
        let open = node(true, false, vcx);
        assert_eq!(open.role(), Role::Button);
        assert_eq!(open.is_expanded(), Some(true));
        assert_eq!(node(false, false, vcx).is_expanded(), Some(false));
        assert!(node(false, true, vcx).is_disabled());
    }
}
