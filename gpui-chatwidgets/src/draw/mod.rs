//! Drawing a card: one function per component, each a GPUI element tree in the host's theme.
//!
//! The rules every function keeps, so no text ever runs out of its box at any width: text is
//! always a text element in a block (never a bare string among a row's flex items), every flex
//! item that holds text can shrink (`min_w_0`), one-line text truncates with an ellipsis inside
//! a definite width, and anything that scrolls sideways says so to the host
//! ([`CardHost::scroller`](crate::CardHost::scroller)).

mod blocks;
mod chart;
mod code;
mod content;
mod form;
mod layout;
mod table;

use gpui_core::Press;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::openui::{Action, ButtonVariant, Card, Cell, Component, Gap, Size, Tone};
use crate::theme::{CardTheme, ToneColors};
use crate::view::CardView;

pub(crate) use form::field_key;

/// Something to press: a row centring what's in it, its text one line tall, as every press in
/// a card is laid out (styles given after it refine these). It takes the focus with Tab,
/// presses with a click, a tap, Enter or Space, and shows the theme's focus ring when the
/// keyboard put the focus there.
pub(crate) fn pressable(id: ElementId, t: &CardTheme) -> Press {
    Press::new(id)
        .focus_ring_color(t.colors.focus_ring)
        .flex()
        .items_center()
        .justify_center()
        .line_height(relative(1.))
}

/// What every drawing function has to hand: the card's view, the theme, the window and the
/// view's context (for listeners).
pub(crate) struct Draw<'a, 'b> {
    pub v: &'a CardView,
    pub t: &'a CardTheme,
    pub window: &'a mut Window,
    pub cx: &'a mut Context<'b, CardView>,
}

/// The card, drawn: its root, in its frame when the theme has one.
pub(crate) fn card(
    v: &CardView,
    t: &CardTheme,
    window: &mut Window,
    cx: &mut Context<CardView>,
) -> AnyElement {
    let mut d = Draw { v, t, window, cx };
    let body = match &v.card {
        Ok(card) => d.component(&card.root, "r"),
        Err(e) => d.note(crate::openui::CardError::SHOWN, Some(&e.to_string())),
    };
    let sel = d.selector("card");
    let root = div()
        .id(d.id("card"))
        .debug_selector(move || sel)
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .font_family(t.fonts.body.clone())
        .text_color(t.colors.text)
        .text_size(t.px(t.text.md))
        .line_height(t.px(t.text.md * t.text.body_leading))
        .child(body);
    if t.frame {
        root.p(t.px(t.space.frame))
            .bg(t.colors.background)
            .border_1()
            .border_color(t.colors.frame_border)
            .rounded(t.px(t.radii.xl))
            .into_any_element()
    } else {
        root.into_any_element()
    }
}

/// The components under a component, each with the path segment the drawing and the
/// preparing both use for it.
pub(crate) fn kids(c: &Component) -> Vec<(String, &Component)> {
    fn list<'c>(prefix: &str, items: &'c [Component]) -> Vec<(String, &'c Component)> {
        items
            .iter()
            .enumerate()
            .map(|(i, k)| (format!("{prefix}{i}"), k))
            .collect()
    }
    match c {
        Component::Stack(f) | Component::Card(f) => list("c", &f.children),
        Component::Modal(m) => list("c", &m.children),
        Component::Tabs(s) | Component::Accordion(s) => s
            .iter()
            .enumerate()
            .flat_map(|(i, sec)| list(&format!("s{i}."), &sec.content))
            .collect(),
        Component::Carousel { slides, .. } => slides
            .iter()
            .enumerate()
            .flat_map(|(i, s)| list(&format!("s{i}."), s))
            .collect(),
        Component::Table(t) => t
            .rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| {
                row.iter()
                    .enumerate()
                    .flat_map(move |(c, cell)| match cell {
                        Cell::Components(v) => list(&format!("t{r}.{c}."), v),
                        _ => Vec::new(),
                    })
            })
            .collect(),
        Component::Cards(b) => b
            .items
            .iter()
            .enumerate()
            .flat_map(|(i, it)| {
                let mut out: Vec<(String, &Component)> =
                    it.header.iter().map(|h| (format!("i{i}h"), &**h)).collect();
                out.extend(list(&format!("i{i}b"), &it.body));
                out.extend(list(&format!("i{i}f"), &it.footer));
                out
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A press found by what it says: its action, its form, and the form's fields when it checks
/// them first (a primary button in a form).
pub(crate) type Found = (
    Action,
    Option<String>,
    Option<Vec<(String, crate::view::Binding)>>,
);

/// The first pressable thing that says `label`.
pub(crate) fn find_button(card: &Card, label: &str) -> Option<Found> {
    fn find(c: &Component, path: &str, label: &str) -> Option<Found> {
        match c {
            Component::Button(b) if b.label == label => {
                return Some((b.action.clone(), None, None));
            }
            Component::IconButton(b) if b.name == label => {
                return Some((b.action.clone(), None, None));
            }
            Component::Buttons(bs) => {
                if let Some(b) = bs.buttons.iter().find(|b| b.label == label) {
                    return Some((b.action.clone(), None, None));
                }
            }
            Component::Form(f) => {
                if let Some(b) = f.buttons.buttons.iter().find(|b| b.label == label) {
                    let fields = f
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, fc)| {
                            (
                                field_key(path, i),
                                crate::view::Binding {
                                    form: Some(f.name.clone()),
                                    field: fc.field.clone(),
                                },
                            )
                        })
                        .collect();
                    return Some((
                        b.action.clone(),
                        Some(f.name.clone()),
                        (b.variant == ButtonVariant::Primary).then_some(fields),
                    ));
                }
            }
            Component::List(l) => {
                if let Some(i) = l
                    .items
                    .iter()
                    .find(|i| i.title == label && i.action.is_some())
                {
                    return Some((i.action.clone()?, None, None));
                }
            }
            Component::Cards(b) => {
                if let Some(p) = b
                    .items
                    .iter()
                    .filter_map(|i| i.press.as_ref())
                    .find(|p| p.words == label)
                {
                    return Some((p.action.clone(), None, None));
                }
            }
            _ => {}
        }
        kids(c)
            .into_iter()
            .find_map(|(seg, k)| find(k, &format!("{path}.{seg}"), label))
    }
    find(&card.root, "r", label)
}

impl Draw<'_, '_> {
    /// An element id under the card's prefix.
    pub fn id(&self, path: &str) -> ElementId {
        ElementId::Name(format!("{}/{path}", self.v.id).into())
    }

    /// A debug selector under the card's prefix (what tests and a host's overflow test find).
    pub fn selector(&self, what: &str) -> String {
        format!("{}:{what}", self.v.id)
    }

    /// A size in CSS pixels, as rems.
    pub fn px(&self, css: f32) -> Rems {
        self.t.px(css)
    }

    /// A size in CSS pixels, in the window's pixels now (for painting).
    pub fn pixels(&self, css: f32) -> Pixels {
        self.t.px(css).to_pixels(self.window.rem_size())
    }

    pub fn gap(&self, g: Gap) -> f32 {
        let s = &self.t.space;
        match g {
            Gap::None => 0.,
            Gap::Xs => s.xs,
            Gap::S => s.s,
            Gap::M => s.m,
            Gap::L => s.l,
            Gap::Xl => s.xl,
            Gap::Xxl => s.xxl,
        }
    }

    pub fn tone(&self, tone: Tone) -> ToneColors {
        let c = &self.t.colors;
        match tone {
            Tone::Info => c.info,
            Tone::Success => c.success,
            Tone::Warning => c.warning,
            Tone::Error | Tone::Danger => c.danger,
            Tone::Neutral | Tone::Soft | Tone::Filled | Tone::Inverted => ToneColors {
                text: c.text,
                background: c.sunk,
                emphasis: c.text,
            },
        }
    }

    /// Text that wraps, in a block that can shrink.
    pub fn text(&self, s: impl Into<SharedString>, size: f32, color: Hsla, weight: f32) -> Div {
        div()
            .min_w_0()
            .text_size(self.px(size))
            .line_height(self.px(size * self.t.text.body_leading))
            .text_color(color)
            .font_weight(FontWeight(weight))
            .child(s.into())
    }

    /// One line, cut short with an ellipsis where it doesn't fit.
    pub fn line(&self, s: impl Into<SharedString>, size: f32, color: Hsla, weight: f32) -> Div {
        div()
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(self.px(size))
            .line_height(self.px(size * self.t.text.label_leading))
            .text_color(color)
            .font_weight(FontWeight(weight))
            .child(s.into())
    }

    /// One line at body leading (1.5 by default), as the web's table cells set their text.
    pub fn body_line(
        &self,
        s: impl Into<SharedString>,
        size: f32,
        color: Hsla,
        weight: f32,
    ) -> Div {
        self.line(s, size, color, weight)
            .line_height(self.px(size * self.t.text.body_leading))
    }

    /// Up to `n` lines, then an ellipsis.
    pub fn clamp(
        &self,
        s: impl Into<SharedString>,
        n: usize,
        size: f32,
        color: Hsla,
        weight: f32,
    ) -> Div {
        self.text(s, size, color, weight)
            .w_full()
            .overflow_hidden()
            .line_clamp(n)
            .text_ellipsis()
    }

    pub fn regular(&self) -> f32 {
        400.
    }

    /// A quiet note in the card's place, or under something cut short.
    pub fn note(&mut self, text: &str, detail: Option<&str>) -> AnyElement {
        let t = self.t;
        let sel = self.selector("note");
        div()
            .debug_selector(move || sel)
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(2.))
            .px(self.px(12.))
            .py(self.px(10.))
            .rounded(self.px(t.radii.md))
            .bg(t.colors.sunk)
            .child(self.text(
                text.to_string(),
                t.text.sm,
                t.colors.text_muted,
                self.regular(),
            ))
            .when_some(detail, |d, s| {
                d.child(self.text(
                    s.to_string(),
                    t.text.xs,
                    t.colors.text_faint,
                    self.regular(),
                ))
            })
            .into_any_element()
    }

    /// Children in a column with a gap, each its own path.
    pub fn column(&mut self, items: &[Component], path: &str, prefix: &str, gap: f32) -> Div {
        let mut col = div().w_full().min_w_0().flex().flex_col().gap(self.px(gap));
        for (i, c) in items.iter().enumerate() {
            col = col.child(self.component(c, &format!("{path}.{prefix}{i}")));
        }
        col
    }

    /// Marks an element that scrolls or clips sideways, for the host's overflow test.
    pub fn sideways<E: InteractiveElement>(&self, el: E, what: &str) -> E {
        let sel = format!("field:{}", self.selector(what));
        self.v.host.scroller(&sel);
        el.debug_selector(move || sel)
    }

    /// A region that scrolls sideways (`id` under the card's prefix), marked for the host's
    /// overflow test as `what`. For touch it moves only with a sideways pan, so an up-and-down
    /// one scrolls the chat instead of sliding the region along; [`Draw::keep_sideways`] on
    /// something around it keeps the sideways pans it scrolls with.
    pub fn scroll_x(&mut self, id: &str, what: &str) -> (Stateful<Div>, Option<ScrollHandle>) {
        let el = div().id(self.id(id)).w_full().min_w_0().overflow_x_scroll();
        let el = self.sideways(el, what);
        if !self.t.touch {
            return (el, None);
        }
        let handle = self
            .window
            .use_keyed_state(self.id(&format!("{id}.handle")), self.cx, |_, _| {
                ScrollHandle::new()
            })
            .read(self.cx)
            .clone();
        (
            el.restrict_scroll_to_axis().track_scroll(&handle),
            Some(handle),
        )
    }

    /// Keeps a finger's sideways pans over `el` from going further (to a carousel turning its
    /// slide, or a chat that would scroll with them) while the region `handle` scrolls can
    /// move. `el` is around the region, so the region has scrolled by then.
    pub fn keep_sideways<E: InteractiveElement>(&self, el: E, handle: Option<ScrollHandle>) -> E {
        let Some(handle) = handle else {
            return el;
        };
        el.on_scroll(move |e, _, cx| {
            if e.source == ScrollSource::Touch
                && !e.delta.pixel_delta(px(1.)).x.is_zero()
                && handle.max_offset().x > px(0.)
            {
                cx.stop_propagation();
            }
        })
    }

    /// Hands a press what it does. When the theme's `min_hit` is larger than the press may be
    /// drawn, it takes presses across a `min_hit` square centred on it too (gpui-core's
    /// [`Press::min_hit`]), so a finger can hit it while it looks and lays out the same.
    pub fn on_press(
        &self,
        press: Press,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Press {
        press
            .when(self.t.min_hit > 0., |p| p.min_hit(px(self.t.min_hit)))
            .on_click(handler)
    }

    /// One component, by kind.
    pub fn component(&mut self, c: &Component, path: &str) -> AnyElement {
        match c {
            Component::Stack(f) => self.flex(f, path, false),
            Component::Card(f) => self.flex(f, path, true),
            Component::Tabs(s) => self.tabs(s, path),
            Component::Accordion(s) => self.accordion(s, path),
            Component::Steps(s) => self.steps(s),
            Component::Carousel { slides, surface } => self.carousel(slides, *surface, path),
            Component::Separator { vertical } => self.separator(*vertical),
            Component::Modal(m) => self.modal(m, path),
            Component::Header { title, subtitle } => {
                self.header(title.as_deref(), subtitle.as_deref())
            }
            Component::Text { markdown, size } => self.text_content(markdown, *size, path),
            Component::Markdown { markdown, surface } => {
                self.markdown_block(markdown, *surface, path)
            }
            Component::Callout(c) => self.callout(c, path),
            Component::Image(i) => self.image(i.src.as_ref(), i.alt.as_deref(), i.block, path),
            Component::Gallery(g) => self.gallery(g, path),
            Component::Code { language, code } => self.code(language, code, path),
            Component::InlineHeader {
                heading,
                description,
            } => self.inline_header(heading, description.as_deref()),
            Component::Table(t) => self.table(t, path),
            Component::EditableTable(t) => self.editable_table(t, path),
            Component::Chart(c) => self.chart(c, path),
            Component::Form(f) => self.form(f, path),
            Component::Field(f) => self.loose_field(f, path),
            Component::Label(s) => self
                .text(
                    s.clone(),
                    self.t.text.sm,
                    self.t.colors.text,
                    self.t.text.medium,
                )
                .into_any_element(),
            Component::Button(b) => self.button(b, path, None, None).into_any_element(),
            Component::Buttons(b) => self.buttons(b, path, None, None),
            Component::IconButton(b) => self.icon_button(b, path),
            Component::TagBlock { tags, size } => self.tag_block(tags, *size),
            Component::Tag(t) => self
                .tag(&t.text, t.icon.as_ref(), t.size, t.tone)
                .into_any_element(),
            Component::Icon(i) => self.icon_element(&i.name, 20., self.t.colors.text),
            Component::EntityList(e) => self.entity_list(e),
            Component::List(l) => self.list(l, path),
            Component::Cards(b) => self.cards(b, path),
            Component::Line(l) => self.line_component(l),
            Component::IconText(i) => self.icon_text(i),
            Component::ImageText(i) => self.image_text(i, path),
            Component::Metric(m) => self.metric(m),
            Component::Plain(s) => self
                .text(
                    s.clone(),
                    self.t.text.md,
                    self.t.colors.text,
                    self.regular(),
                )
                .into_any_element(),
            Component::Unknown(_) => div().into_any_element(),
        }
    }

    /// A Lucide icon through the host, or nothing when the host has none.
    pub fn icon_element(&self, name: &str, size: f32, color: Hsla) -> AnyElement {
        match self.v.host.icon(name, self.cx) {
            Some(path) => svg()
                .path(path)
                .flex_none()
                .size(self.px(size))
                .text_color(color)
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// A tag's text size and box for a size keyword.
    pub fn tag_metrics(&self, size: Size) -> (f32, f32, f32) {
        // (text, horizontal padding, height)
        match size {
            Size::Lg | Size::Large | Size::L | Size::Xl => (self.t.text.sm, 12., 36.),
            Size::Md | Size::Medium | Size::M | Size::Default => (self.t.text.sm, 8., 32.),
            _ => (self.t.text.sm, 6., 24.),
        }
    }
}
