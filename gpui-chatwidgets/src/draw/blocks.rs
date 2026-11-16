//! Card blocks: grids (or a carousel) of small cards — snippet, overview, context, composite
//! and visual — each pressable when its block has an action.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::content::{Glyph, img_from};
use super::{Draw, pressable};
use crate::openui::{CardBlock, CardBlockKind, CardItem};

impl Draw<'_, '_> {
    pub fn cards(&mut self, b: &CardBlock, path: &str) -> AnyElement {
        let t = self.t;
        let shown = b.items.len().min(self.v.limits.list_items);
        if shown == 0 {
            return div().into_any_element();
        }
        if b.carousel {
            let slide = self.v.ui.carousel.get(path).copied().unwrap_or_default();
            let index = slide.index.min(shown - 1);
            let card = self.card_item(b.kind, &b.items[index], &format!("{path}.i{index}"));
            let moving = self.slide_in(div().w_full().min_w_0().child(card), path, slide, index);
            let clip = self
                .sideways(
                    div()
                        .id(self.id(&format!("{path}.clip")))
                        .w_full()
                        .min_w_0()
                        .overflow_hidden(),
                    &format!("{path}.clip"),
                )
                .child(moving);
            let pager = self.pager(path, index, shown);
            return div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(self.px(t.space.s))
                .child(clip)
                .when(shown > 1, |d| d.child(pager))
                .into_any_element();
        }
        let min = match b.kind {
            CardBlockKind::Snippet => 240.,
            CardBlockKind::Overview => 158.,
            CardBlockKind::Context => 196.,
            CardBlockKind::Composite => 240.,
            CardBlockKind::Visual => 200.,
        };
        let mut grid = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(self.px(t.space.m));
        for (i, it) in b.items.iter().take(shown).enumerate() {
            let card = self.card_item(b.kind, it, &format!("{path}.i{i}"));
            grid = grid.child(
                div()
                    .flex_basis(self.px(min))
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .max_w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(card),
            );
        }
        grid.into_any_element()
    }

    fn card_item(&mut self, kind: CardBlockKind, it: &CardItem, path: &str) -> AnyElement {
        let t = self.t;
        let clickable = it.press.is_some();
        let header = it
            .header
            .as_ref()
            .map(|h| self.component(h, &format!("{path}h")));
        let body: Vec<AnyElement> = it
            .body
            .iter()
            .enumerate()
            .map(|(j, c)| self.component(c, &format!("{path}b{j}")))
            .collect();
        let footer: Vec<AnyElement> = it
            .footer
            .iter()
            .enumerate()
            .map(|(j, c)| self.component(c, &format!("{path}f{j}")))
            .collect();
        let chevron = || self.glyph(Glyph::ChevronRight, 16., t.colors.text_muted);
        let surface = |d: Div| {
            d.w_full()
                .min_w_0()
                .border_1()
                .border_color(if clickable {
                    t.colors.line_strong
                } else {
                    t.colors.line
                })
                .bg(if clickable {
                    t.colors.surface
                } else {
                    t.colors.highlight
                })
        };
        let inner: Div = match kind {
            CardBlockKind::Snippet => surface(div())
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(self.px(t.space.m))
                .p(self.px(10.))
                .rounded(self.px(14.))
                .child(div().flex_1().min_w_0().children(header))
                .child(
                    div()
                        .flex_shrink(1.)
                        .max_w(relative(0.5))
                        .min_w_0()
                        .flex()
                        .justify_end()
                        .children(body),
                )
                .when(clickable, |d| d.child(chevron())),
            CardBlockKind::Overview => surface(div())
                .flex()
                .flex_col()
                .justify_between()
                .gap(self.px(t.space.l))
                .p(self.px(10.))
                .rounded(self.px(14.))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_start()
                        .justify_between()
                        .gap(self.px(t.space.s))
                        .child(div().flex_1().min_w_0().children(header))
                        .when(clickable, |d| d.child(chevron())),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(self.px(4.))
                        .children(body),
                ),
            CardBlockKind::Context => {
                let mut d = div()
                    .w_full()
                    .min_w_0()
                    .relative()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(self.px(6.))
                    .p(self.px(t.space.m))
                    .rounded(self.px(t.radii.xl))
                    .bg(if it.gray || clickable {
                        t.colors.highlight
                    } else {
                        t.colors.sunk
                    });
                if let Some(img) = self.picture(it.background.as_ref()) {
                    d = d.child(
                        div().absolute().inset_0().opacity(0.25).child(
                            img_from(img)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .rounded(self.px(t.radii.xl)),
                        ),
                    );
                }
                if let Some(tag) = &it.tag {
                    d = d.child(div().w_full().min_w_0().flex().child(self.tag(
                        &tag.text,
                        tag.icon.as_ref(),
                        tag.size,
                        tag.tone,
                    )));
                } else if let Some(title) = &it.title {
                    d = d.child(self.text(
                        title.clone(),
                        t.text.sm,
                        t.colors.text_muted,
                        self.regular(),
                    ));
                }
                if let Some(text) = &it.text {
                    d = d.child(self.markdown(
                        text,
                        t.text.md,
                        self.regular(),
                        &format!("{path}.text"),
                    ));
                }
                d
            }
            CardBlockKind::Composite => surface(div())
                .flex()
                .flex_col()
                .gap(self.px(t.space.m))
                .p(self.px(t.space.m))
                .rounded(self.px(t.radii.card))
                .children(header)
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(self.px(t.space.m))
                        .children(body),
                )
                .when(!footer.is_empty(), |d| {
                    d.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(self.px(t.space.s))
                            .children(footer),
                    )
                }),
            CardBlockKind::Visual => {
                let mut d = div()
                    .w_full()
                    .min_w_0()
                    .min_h(self.px(220.))
                    .relative()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .rounded(self.px(t.radii.card))
                    .border_1()
                    .border_color(t.colors.line)
                    .bg(t.colors.highlight);
                if let Some(img) = self.picture(it.background.as_ref()) {
                    d = d.child(
                        div().absolute().inset_0().child(
                            img_from(img)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .rounded(self.px(t.radii.card)),
                        ),
                    );
                }
                let top = div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .p(self.px(t.space.m))
                    .flex()
                    .flex_row()
                    .justify_between()
                    .when_some(it.tag.as_ref(), |d, tag| {
                        d.child(self.tag(&tag.text, tag.icon.as_ref(), tag.size, tag.tone))
                    });
                let bottom = div()
                    .relative()
                    .mx(self.px(t.space.m))
                    .mb(self.px(t.space.m))
                    .px(self.px(10.))
                    .py(self.px(8.))
                    .rounded(self.px(t.radii.lg))
                    .bg(t.colors.background)
                    .border_1()
                    .border_color(t.colors.line)
                    .min_w_0()
                    .children(body);
                d.child(top).child(bottom)
            }
        };
        match &it.press {
            Some(p) => {
                let (action, words) = (p.action.clone(), p.words.clone());
                pressable(self.id(&format!("{path}.press")), self.t)
                    .accessibility_label(p.words.clone())
                    .w_full()
                    .min_w_0()
                    .flex_col()
                    .items_stretch()
                    .justify_start()
                    .rounded(self.px(14.))
                    .hover(|s| s.opacity(0.92))
                    .debug_selector({
                        let sel = self.selector(&format!("card:{}", p.words));
                        move || sel
                    })
                    .child(inner)
                    .on_click(self.cx.listener(move |this, _, window, cx| {
                        this.press(&action, &words, None, None, window, cx)
                    }))
                    .into_any_element()
            }
            None => inner.into_any_element(),
        }
    }
}
