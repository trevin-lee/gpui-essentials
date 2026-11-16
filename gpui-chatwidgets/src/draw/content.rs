//! Content: headers, text and Markdown, callouts, images, tags, entity lists, lists, and the
//! small text blocks the card blocks are made of; and the few glyphs the card draws itself
//! (chevrons, a close mark, a check), so it needs no icons from the host for its own controls.

use std::sync::Arc;

use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Draw;
use crate::openui::{
    Callout, EntityList, GalleryImage, Icon, IconText, ImageSrc, ImageText, Line, ListBlock,
    Metric, Size, Surface, TextSize, Tone,
};
use crate::view::ImageState;

/// A glyph the card draws with lines of its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Glyph {
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Close,
    Check,
    Copy,
    Picture,
    ArrowUp,
    ArrowDown,
}

/// A glyph drawn as strokes, turned by an angle (so it can turn smoothly).
#[derive(IntoElement)]
pub(crate) struct GlyphElement {
    glyph: Glyph,
    size: Rems,
    color: Hsla,
    angle: f32,
}

impl GlyphElement {
    /// The glyph turned by `angle` radians about its centre.
    pub fn turned(mut self, angle: f32) -> Self {
        self.angle = angle;
        self
    }
}

impl RenderOnce for GlyphElement {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (glyph, color, angle) = (self.glyph, self.color, self.angle);
        canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let s = f32::from(b.size.width.min(b.size.height));
                let (cx, cy) = (
                    f32::from(b.origin.x) + f32::from(b.size.width) / 2.,
                    f32::from(b.origin.y) + f32::from(b.size.height) / 2.,
                );
                let (sin, cos) = angle.sin_cos();
                // Points in a unit square centred on 0, turned, then placed.
                let at = |x: f32, y: f32| {
                    let (x, y) = (x * cos - y * sin, x * sin + y * cos);
                    point(px(cx + x * s), px(cy + y * s))
                };
                let lines: &[&[(f32, f32)]] = match glyph {
                    Glyph::ChevronDown => &[&[(-0.25, -0.12), (0., 0.13), (0.25, -0.12)]],
                    Glyph::ChevronLeft => &[&[(0.12, -0.25), (-0.13, 0.), (0.12, 0.25)]],
                    Glyph::ChevronRight => &[&[(-0.12, -0.25), (0.13, 0.), (-0.12, 0.25)]],
                    Glyph::Close => &[
                        &[(-0.25, -0.25), (0.25, 0.25)],
                        &[(0.25, -0.25), (-0.25, 0.25)],
                    ],
                    Glyph::Check => &[&[(-0.3, 0.0), (-0.08, 0.22), (0.3, -0.2)]],
                    Glyph::Copy => &[
                        &[(-0.12, -0.3), (0.32, -0.3), (0.32, 0.14)],
                        &[
                            (-0.32, -0.14),
                            (0.14, -0.14),
                            (0.14, 0.32),
                            (-0.32, 0.32),
                            (-0.32, -0.14),
                        ],
                    ],
                    Glyph::Picture => &[
                        &[
                            (-0.36, -0.28),
                            (0.36, -0.28),
                            (0.36, 0.28),
                            (-0.36, 0.28),
                            (-0.36, -0.28),
                        ],
                        &[
                            (-0.36, 0.2),
                            (-0.1, -0.04),
                            (0.08, 0.12),
                            (0.2, 0.02),
                            (0.36, 0.16),
                        ],
                    ],
                    Glyph::ArrowUp => &[
                        &[(0., 0.3), (0., -0.3)],
                        &[(-0.22, -0.08), (0., -0.3), (0.22, -0.08)],
                    ],
                    Glyph::ArrowDown => &[
                        &[(0., -0.3), (0., 0.3)],
                        &[(-0.22, 0.08), (0., 0.3), (0.22, 0.08)],
                    ],
                };
                for line in lines {
                    let mut p = PathBuilder::stroke(px((s * 0.11).max(1.2)));
                    for (i, (x, y)) in line.iter().enumerate() {
                        if i == 0 {
                            p.move_to(at(*x, *y));
                        } else {
                            p.line_to(at(*x, *y));
                        }
                    }
                    if let Ok(path) = p.build() {
                        window.paint_path(path, color);
                    }
                }
            },
        )
        .flex_none()
        .size(self.size)
    }
}

/// Text for the text view: HTML in it is shown as the characters it is (OpenUI's web renderer
/// never renders it either), and images become their alt text.
pub(crate) fn markdown_source(md: &str) -> String {
    without_images(md).replace('<', "\\<")
}

/// Markdown images aren't loaded: each becomes its alt text before the text view sees it.
pub(crate) fn without_images(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(at) = rest.find("![") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(close) = after.find(']') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let alt = &after[..close];
        let tail = &after[close + 1..];
        let end = match tail.chars().next() {
            Some('(') => tail.find(')').map(|e| e + 1),
            Some('[') => tail.find(']').map(|e| e + 1),
            _ => Some(0),
        };
        match end {
            Some(e) => {
                out.push_str(alt);
                rest = &tail[e..];
            }
            None => {
                out.push_str(alt);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

impl Draw<'_, '_> {
    pub fn glyph(&self, glyph: Glyph, size: f32, color: Hsla) -> GlyphElement {
        GlyphElement {
            glyph,
            size: self.px(size),
            color,
            angle: 0.,
        }
    }

    /// A card's title and subtitle.
    pub fn header(&mut self, title: Option<&str>, subtitle: Option<&str>) -> AnyElement {
        let t = self.t;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.xs))
            .mb(self.px(4.))
            .when_some(title, |d, s| {
                d.child(
                    self.text(
                        s.to_string(),
                        t.text.heading,
                        t.colors.text,
                        t.text.semibold,
                    )
                    .line_height(self.px(t.text.heading * 1.25)),
                )
            })
            .when_some(subtitle, |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.md,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }

    pub fn inline_header(&mut self, heading: &str, description: Option<&str>) -> AnyElement {
        let t = self.t;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(2.))
            .child(self.text(heading.to_string(), t.text.md, t.colors.text, t.text.medium))
            .when_some(description, |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.md,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }

    /// Markdown as text, selectable, through GPUI Kit's text view: no HTML, no images, and a
    /// link goes to the host as an event.
    pub fn markdown(&mut self, md: &str, size: f32, weight: f32, path: &str) -> Div {
        let t = self.t;
        let rem = self.window.rem_size();
        let heading = t.px(t.text.md).to_pixels(rem);
        let mut code = StyleRefinement::default()
            .bg(t.colors.sunk)
            .border_1()
            .border_color(t.colors.line)
            .rounded(self.px(t.radii.md))
            .px(self.px(12.))
            .py(self.px(10.));
        code.text = TextStyleRefinement {
            font_family: Some(t.fonts.mono.clone()),
            font_size: Some(self.px(t.text.code).into()),
            line_height: Some(self.px(t.text.code * 1.6).into()),
            ..Default::default()
        };
        let style = TextViewStyle {
            paragraph_gap: self.px(size * 0.75),
            heading_base_font_size: heading,
            heading_font_size: Some(Arc::new(|_, base| base)),
            code_block: code,
            ..Default::default()
        };
        let view = self.cx.entity().downgrade();
        let text = TextView::markdown(self.id(&format!("{path}.md")), markdown_source(md))
            .style(style)
            .selectable(true)
            .stream_fade(false)
            .image_source(|_| {
                ImageSource::Custom(Arc::new(|_, _| {
                    Some(Err(ImageCacheError::Io(Arc::new(std::io::Error::other(
                        "a card's Markdown images aren't loaded",
                    )))))
                }))
            })
            .on_link_click(move |url, _, _, cx| {
                let url = url.to_string();
                let _ = view.update(cx, |v, cx| v.open_markdown_link(&url, cx));
            });
        div()
            .w_full()
            .min_w_0()
            .text_size(self.px(size))
            .line_height(self.px(size * t.text.body_leading))
            .font_weight(FontWeight(weight))
            .text_color(t.colors.text)
            .child(text)
    }

    pub fn text_content(&mut self, md: &str, size: TextSize, path: &str) -> AnyElement {
        let t = self.t;
        let (s, w) = match size {
            TextSize::Small => (t.text.sm, self.regular()),
            TextSize::Large => (t.text.lg, self.regular()),
            TextSize::SmallHeavy => (t.text.sm, t.text.medium),
            TextSize::LargeHeavy => (t.text.lg, t.text.medium),
            TextSize::Default => (t.text.md, self.regular()),
        };
        self.markdown(md, s, w, path).into_any_element()
    }

    pub fn markdown_block(&mut self, md: &str, surface: Surface, path: &str) -> AnyElement {
        let t = self.t;
        let body = self.markdown(md, t.text.md, self.regular(), path);
        match surface {
            Surface::Clear => body,
            Surface::Card => body
                .p(self.px(16.))
                .rounded(self.px(t.radii.md))
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.line),
            Surface::Sunk => body
                .p(self.px(16.))
                .rounded(self.px(t.radii.md))
                .bg(t.colors.sunk),
        }
        .into_any_element()
    }

    /// A banner with a bar in its tone. One bound to a $variable shows while it's true.
    pub fn callout(&mut self, c: &Callout, path: &str) -> AnyElement {
        let t = self.t;
        if let Some(visible) = &c.visible {
            let shown = match visible {
                crate::openui::Bound::Var(name) => self
                    .v
                    .store
                    .var(name)
                    .is_some_and(|v| crate::view::truthy(&v)),
                crate::openui::Bound::Fixed(v) => crate::view::truthy(v),
            };
            if !shown {
                return div().into_any_element();
            }
        }
        // OpenUI's Callout draws an error as a warning; its TextCallout has a danger tone.
        let tone = if !c.plain && c.tone == Tone::Error {
            Tone::Warning
        } else {
            c.tone
        };
        let colors = self.tone(tone);
        let (title_color, bar) = match tone {
            Tone::Neutral | Tone::Soft | Tone::Filled | Tone::Inverted => {
                (t.colors.text, t.colors.text)
            }
            _ => (colors.text, colors.emphasis),
        };
        let mut col = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .px(self.px(if c.plain { 16. } else { 14. }))
            .border_l_2()
            .border_color(bar);
        if let Some(title) = &c.title {
            col = col.child(self.text(title.clone(), t.text.md, title_color, t.text.medium));
        }
        if let Some(d) = &c.description {
            col = if c.plain {
                col.child(self.text(d.clone(), t.text.md, t.colors.text, self.regular()))
            } else {
                col.child(self.markdown(d, t.text.md, self.regular(), &format!("{path}.d")))
            };
        }
        col.into_any_element()
    }

    /// An image the host sent, or its alt text where it has none.
    pub fn picture(&self, src: Option<&ImageSrc>) -> Option<Arc<Image>> {
        match src.and_then(|s| self.v.images.get(s)) {
            Some(ImageState::Ready(img)) => Some(img.clone()),
            _ => None,
        }
    }

    /// The box an image that isn't shown leaves: an outline glyph and the alt text.
    pub fn alt_box(&self, alt: Option<&str>) -> Div {
        let t = self.t;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap(self.px(8.))
            .rounded(self.px(t.radii.md))
            .bg(t.colors.sunk)
            .px(self.px(12.))
            .py(self.px(10.))
            .child(self.glyph(Glyph::Picture, 16., t.colors.text_muted))
            .child(
                self.text(
                    alt.unwrap_or("An image that didn't load.").to_string(),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                )
                .flex_1(),
            )
    }

    pub fn image(
        &mut self,
        src: Option<&ImageSrc>,
        alt: Option<&str>,
        block: bool,
        path: &str,
    ) -> AnyElement {
        let t = self.t;
        let _ = path;
        match self.picture(src) {
            Some(img) => {
                let radius = self.px(if block { t.radii.xl } else { t.radii.md });
                let pic = img_from(img)
                    .w_full()
                    .max_h(self.px(if block { 240. } else { 480. }))
                    .object_fit(ObjectFit::Contain)
                    .rounded(radius);
                div()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(self.px(if block { t.radii.xl } else { t.radii.md }))
                    .border_1()
                    .border_color(t.colors.line)
                    .child(pic)
                    .into_any_element()
            }
            None => self.alt_box(alt).into_any_element(),
        }
    }

    /// Thumbnails in a wrapping grid, each with its details below.
    pub fn gallery(&mut self, images: &[GalleryImage], path: &str) -> AnyElement {
        let t = self.t;
        let shown = images.len().min(self.v.limits.gallery_images);
        let mut grid = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(self.px(t.space.s));
        for (i, g) in images.iter().take(shown).enumerate() {
            let pic = match self.picture(g.src.as_ref()) {
                Some(img) => div()
                    .w_full()
                    .h(self.px(120.))
                    .overflow_hidden()
                    .rounded(self.px(t.radii.md))
                    .child(
                        img_from(img)
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .rounded(self.px(t.radii.md)),
                    ),
                None => self.alt_box(g.alt.as_deref()).min_h(self.px(64.)),
            };
            let _ = i;
            grid = grid.child(
                div()
                    .flex_basis(self.px(120.))
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .max_w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(self.px(4.))
                    .child(pic)
                    .when_some(g.details.as_deref(), |d, s| {
                        d.child(self.clamp(
                            s.to_string(),
                            2,
                            t.text.sm,
                            t.colors.text_muted,
                            self.regular(),
                        ))
                    }),
            );
        }
        let _ = path;
        let more = images.len() - shown;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.s))
            .child(grid)
            .when(more > 0, |d| {
                d.child(self.text(
                    format!("{more} more images not shown."),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }

    /// A small rounded label in its tone, cut short when it doesn't fit.
    pub fn tag(&self, text: &str, icon: Option<&Icon>, size: Size, tone: Tone) -> Div {
        let t = self.t;
        let colors = self.tone(tone);
        let (font, pad, height) = self.tag_metrics(size);
        div()
            .flex_none()
            .max_w_full()
            .min_w_0()
            .h(self.px(height))
            .px(self.px(pad))
            .flex()
            .flex_row()
            .items_center()
            .gap(self.px(4.))
            .rounded(self.px(if height > 32. { t.radii.md } else { t.radii.sm }))
            .bg(colors.background)
            .when_some(icon, |d, i| {
                d.child(self.icon_element(&i.name, 14., colors.text))
            })
            .child(self.line(text.to_string(), font, colors.text, self.regular()))
    }

    pub fn tag_block(&mut self, tags: &[String], size: Size) -> AnyElement {
        let t = self.t;
        let shown = tags.len().min(self.v.limits.list_items);
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(self.px(t.space.s));
        for tag in &tags[..shown] {
            row = row.child(self.tag(tag, None, size, Tone::Neutral));
        }
        if tags.len() > shown {
            row = row.child(self.tag(
                &format!("+{}", tags.len() - shown),
                None,
                size,
                Tone::Neutral,
            ));
        }
        row.into_any_element()
    }

    /// Label and value rows; the header muted, the footer's value larger.
    pub fn entity_list(&mut self, e: &EntityList) -> AnyElement {
        let t = self.t;
        let size = if e.small { t.text.sm } else { t.text.md };
        let rows: Vec<(&crate::openui::Entity, u8)> = e
            .header
            .iter()
            .map(|r| (r, 0))
            .chain(e.rows.iter().take(self.v.limits.list_items).map(|r| (r, 1)))
            .chain(e.footer.iter().map(|r| (r, 2)))
            .collect();
        let n = rows.len();
        let mut col = div().w_full().min_w_0().flex().flex_col();
        for (i, (r, kind)) in rows.into_iter().enumerate() {
            let color = if kind == 0 {
                t.colors.text_muted
            } else {
                t.colors.text
            };
            let right_size = if kind == 2 && !e.small {
                t.text.lg
            } else {
                size
            };
            let row = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(self.px(t.space.m))
                .py(self.px(t.space.m))
                .when(i + 1 < n, |d| d.border_b_1().border_color(t.colors.line))
                .child(
                    self.text(r.left.clone(), size, color, self.regular())
                        .flex_1(),
                )
                .child(
                    self.text(
                        r.right.clone(),
                        right_size,
                        color,
                        if kind == 0 {
                            self.regular()
                        } else {
                            t.text.medium
                        },
                    )
                    .flex_1()
                    .text_right(),
                );
            col = col.child(row);
        }
        col.into_any_element()
    }

    /// Numbered (or pictured) items with a title and subtitle; one with an action is a
    /// button that sends its title.
    pub fn list(&mut self, l: &ListBlock, path: &str) -> AnyElement {
        let t = self.t;
        let size = if l.small { t.text.sm } else { t.text.md };
        let shown = l.items.len().min(self.v.limits.list_items);
        let mut col = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(if l.small { t.space.m } else { t.space.l }));
        for (i, it) in l.items.iter().take(shown).enumerate() {
            let side = match (it.subtitle.is_some(), l.small) {
                (false, true) => 20.,
                (false, false) => 24.,
                (true, true) => 36.,
                (true, false) => 40.,
            };
            let mark = div()
                .flex_none()
                .size(self.px(side))
                .rounded(self.px(if it.subtitle.is_some() {
                    t.radii.md
                } else {
                    t.radii.sm
                }))
                .bg(t.colors.sunk)
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center();
            let mark = match it
                .image
                .as_ref()
                .and_then(|im| self.picture(im.src.as_ref()))
            {
                Some(img) => mark.child(
                    img_from(img)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .rounded(self.px(t.radii.sm)),
                ),
                None => mark.child(self.line(
                    (i + 1).to_string(),
                    if l.small { t.text.sm } else { t.text.md },
                    t.colors.text_muted,
                    self.regular(),
                )),
            };
            let words = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(self.text(it.title.clone(), size, t.colors.text, self.regular()))
                .when_some(it.subtitle.as_deref(), |d, s| {
                    d.child(self.text(s.to_string(), size, t.colors.text_muted, self.regular()))
                });
            let row = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_row()
                .gap(self.px(if l.small { 10. } else { t.space.m }))
                .when(it.subtitle.is_none(), |d| d.items_center())
                .child(mark.when(it.subtitle.is_some(), |m| m.mt(self.px(4.))))
                .child(words);
            match &it.action {
                Some(action) => {
                    let action_side = div()
                        .flex_none()
                        .max_w(relative(0.4))
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(self.px(4.))
                        .when_some(it.action_label.as_deref(), |d, s| {
                            d.child(self.line(
                                s.to_string(),
                                size,
                                t.colors.text_muted,
                                self.regular(),
                            ))
                        })
                        .child(self.glyph(Glyph::ChevronRight, 16., t.colors.text_muted));
                    let (action, words) = (action.clone(), it.title.clone());
                    let go = self.cx.listener(move |this, _, window, cx| {
                        this.press(&action, &words, None, None, window, cx)
                    });
                    let press = gpui_kit::base::Button::new(self.id(&format!("{path}.i{i}")))
                        .accessibility_label(it.title.clone())
                        .w_full()
                        .min_w_0()
                        .justify_start()
                        .rounded(self.px(t.radii.sm))
                        .hover(|s| s.bg(t.colors.highlight))
                        .debug_selector({
                            let sel = self.selector(&format!("item:{}", it.title));
                            move || sel
                        })
                        .child(row.child(action_side));
                    let press = self.on_press(press, true, go);
                    let wrap = div()
                        .w_full()
                        .min_w_0()
                        .when(i + 1 < shown, |d| {
                            d.pb(self.px(t.space.m))
                                .border_b_1()
                                .border_color(t.colors.line)
                        })
                        .child(press);
                    col = col.child(wrap);
                }
                None => col = col.child(row),
            }
        }
        if l.items.len() > shown {
            col = col.child(self.text(
                format!("{} more items not shown.", l.items.len() - shown),
                t.text.sm,
                t.colors.text_muted,
                self.regular(),
            ));
        }
        col.into_any_element()
    }

    /// Text and BoldText: a value with optional subtext; a metric's subtext is green when it
    /// starts with + and red with -.
    pub fn line_component(&mut self, l: &Line) -> AnyElement {
        let t = self.t;
        let (value_size, sub_size, heavy) = match l.size {
            Size::Lg | Size::Large | Size::L | Size::Xl => (24., t.text.md, t.text.semibold),
            Size::Md | Size::Medium | Size::M | Size::Default => {
                (t.text.lg, t.text.sm, t.text.medium)
            }
            Size::Xs | Size::ExtraSmall => (t.text.sm, t.text.xs, t.text.medium),
            _ => (t.text.md, t.text.sm, t.text.medium),
        };
        let sub_color = match (
            l.subtext_kind.as_str(),
            l.subtext.as_deref().map(str::trim_start),
        ) {
            ("metric", Some(s)) if s.starts_with('+') => t.colors.success.text,
            ("metric", Some(s)) if s.starts_with('-') || s.starts_with('−') => {
                t.colors.danger.text
            }
            _ => t.colors.text_muted,
        };
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(2.))
            .child(self.text(
                l.value.clone(),
                value_size,
                t.colors.text,
                if l.bold { heavy } else { self.regular() },
            ))
            .when_some(l.subtext.as_deref(), |d, s| {
                d.child(self.text(s.to_string(), sub_size, sub_color, self.regular()))
            })
            .into_any_element()
    }

    /// An icon in a small tinted square, beside (or above) a title and subtitle.
    pub fn icon_text(&mut self, i: &IconText) -> AnyElement {
        let t = self.t;
        let (side, glyph) = match i.icon_size {
            Size::S | Size::Sm | Size::Small | Size::Xs | Size::ExtraSmall => (24., 14.),
            Size::L | Size::Lg | Size::Large | Size::Xl => (40., 20.),
            _ => (32., 16.),
        };
        let tone = self.tone(i.tone);
        let mark = div()
            .flex_none()
            .size(self.px(side))
            .rounded(self.px(t.radii.sm))
            .bg(tone.background)
            .flex()
            .items_center()
            .justify_center()
            .child(self.icon_element(&i.icon.name, glyph, tone.text));
        let words = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(self.text(
                i.title.clone(),
                t.text.sm,
                t.colors.text,
                if i.bold || !i.vertical {
                    t.text.medium
                } else {
                    self.regular()
                },
            ))
            .when_some(i.subtitle.as_deref(), |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            });
        let row = div().w_full().min_w_0().flex().gap(self.px(t.space.m));
        if i.vertical {
            row.flex_col().items_start()
        } else {
            row.flex_row().items_start()
        }
        .child(mark)
        .child(words)
        .into_any_element()
    }

    pub fn image_text(&mut self, i: &ImageText, path: &str) -> AnyElement {
        let t = self.t;
        let _ = path;
        let words = div()
            .min_w_0()
            .flex()
            .flex_col()
            .child(self.text(
                i.title.clone(),
                t.text.sm,
                t.colors.text,
                if i.bold || !i.vertical {
                    t.text.medium
                } else {
                    self.regular()
                },
            ))
            .when_some(i.subtitle.as_deref(), |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            });
        if i.large {
            let banner = match self.picture(i.src.as_ref()) {
                Some(img) => div()
                    .w_full()
                    .h(self.px(180.))
                    .rounded(self.px(t.radii.md))
                    .overflow_hidden()
                    .bg(t.colors.sunk)
                    .child(
                        img_from(img)
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .rounded(self.px(t.radii.md)),
                    ),
                None => self.alt_box(i.alt.as_deref()),
            };
            return div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(self.px(t.space.m))
                .child(banner)
                .child(words.w_full())
                .into_any_element();
        }
        let side = i
            .image_size
            .filter(|s| s.is_finite() && *s > 0.)
            .map_or(40., |s| s.clamp(16., 96.) as f32);
        let thumb = div()
            .flex_none()
            .size(self.px(side))
            .rounded(self.px(t.radii.md))
            .overflow_hidden()
            .bg(t.colors.sunk)
            .flex()
            .items_center()
            .justify_center();
        let thumb = match self.picture(i.src.as_ref()) {
            Some(img) => thumb.child(
                img_from(img)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(self.px(t.radii.md)),
            ),
            None => {
                thumb.child(self.glyph(Glyph::Picture, (side * 0.45).min(20.), t.colors.text_muted))
            }
        };
        let row = div().w_full().min_w_0().flex().gap(self.px(t.space.m));
        if i.vertical {
            row.flex_col()
                .items_start()
                .child(thumb)
                .child(words.w_full())
        } else {
            row.flex_row()
                .items_center()
                .child(thumb)
                .child(words.flex_1())
        }
        .into_any_element()
    }

    /// A value with its trend (an arrow and a percentage), a struck-through previous value,
    /// and subtext.
    pub fn metric(&mut self, m: &Metric) -> AnyElement {
        let t = self.t;
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_end()
            .gap(self.px(t.space.xs));
        if let Some(p) = &m.previous {
            row = row.child(
                self.text(p.clone(), t.text.sm, t.colors.text_muted, self.regular())
                    .line_through(),
            );
        }
        row = row.child(self.text(m.value.clone(), t.text.md, t.colors.text, t.text.medium));
        if let Some(tr) = m.trend {
            let color = if tr.up {
                t.colors.success.text
            } else {
                t.colors.danger.text
            };
            let pct = crate::openui::num_text(tr.value);
            row = row.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(self.px(2.))
                    .child(self.glyph(
                        if tr.up {
                            Glyph::ArrowUp
                        } else {
                            Glyph::ArrowDown
                        },
                        12.,
                        color,
                    ))
                    .child(self.line(format!("{pct}%"), t.text.sm, color, self.regular())),
            );
        }
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(2.))
            .child(row)
            .when_some(m.subtext.as_deref(), |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }
}

/// An image element from bytes the host gave (never from a URL).
pub(crate) fn img_from(image: Arc<Image>) -> Img {
    img(ImageSource::Image(image))
}

#[cfg(test)]
mod tests {
    use super::{markdown_source, without_images};

    #[test]
    fn markdown_images_become_their_alt_text() {
        assert_eq!(
            without_images("a ![cat](https://evil.test/x.png) b"),
            "a cat b"
        );
        assert_eq!(without_images("![ref][1] and ![](data:x)"), "ref and ");
        assert_eq!(without_images("![open"), "![open");
        assert_eq!(without_images("![alt](no close"), "alt(no close");
        assert_eq!(
            without_images("plain [link](https://example.com)"),
            "plain [link](https://example.com)"
        );
        assert_eq!(
            markdown_source("<script>x</script> ![a](b)"),
            "\\<script>x\\</script> a"
        );
    }
}
