//! TextView: Markdown drawn natively, selectable, with links as events for the host.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AbsoluteLength, App, AvailableSpace, Bounds, ClipboardItem, CursorStyle, DefiniteLength,
    DispatchPhase, Element, ElementId, Fill, FocusHandle, FontWeight, GlobalElementId,
    HitboxBehavior, Hsla, InspectorElementId, InteractiveElement as _, IntoElement, KeyBinding,
    LayoutId, Length, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Point, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Style, StyleRefinement, Styled as _, Window, actions, div,
    fill, point, px, quad, relative, rems, size, transparent_black,
};
use unicode_segmentation::UnicodeSegmentation as _;

use gpui::TextAlign;

use super::markdown::{Align, Block, parse};
use super::paragraph::{Paragraph, ParagraphLayout, ParagraphStyle};
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_text,
    [CopySelection, SelectAllText, ClearSelection]
);

const CONTEXT: &str = "GpuiCoreTextView";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        cx.bind_keys([
            KeyBinding::new("cmd-c", CopySelection, c),
            KeyBinding::new("cmd-a", SelectAllText, c),
        ]);
    } else {
        cx.bind_keys([
            KeyBinding::new("ctrl-c", CopySelection, c),
            KeyBinding::new("ctrl-a", SelectAllText, c),
        ]);
    }
    cx.bind_keys([KeyBinding::new("escape", ClearSelection, c)]);
}

type HeadingSize = Arc<dyn Fn(u8, Pixels) -> Pixels + Send + Sync + 'static>;

/// How a TextView lays its blocks out, beyond the theme.
#[derive(Clone)]
pub struct TextViewStyle {
    /// The space between blocks.
    pub paragraph_gap: Rems,
    /// The size headings are measured from (their text is bold): the size of the text around
    /// the view unless given, so it follows the host's zoom.
    pub heading_base_font_size: Option<Pixels>,
    /// A heading's size from its level (1 to 6) and the base. By default: 1.6, 1.35 and 1.15
    /// times the base for levels 1 to 3, the base for the rest.
    pub heading_font_size: Option<HeadingSize>,
    /// A code block's box: its fill, border, corners, padding and text.
    pub code_block: StyleRefinement,
}

impl Default for TextViewStyle {
    fn default() -> Self {
        Self {
            paragraph_gap: rems(0.75),
            heading_base_font_size: None,
            heading_font_size: None,
            code_block: StyleRefinement::default(),
        }
    }
}

type OnLink = Rc<dyn Fn(&str, &Modifiers, &mut Window, &mut App)>;

/// Markdown, drawn. Nothing in it fetches or opens anything: a link press goes to
/// [`on_link_click`](TextView::on_link_click), and an image shows as its description.
#[derive(IntoElement)]
pub struct TextView {
    id: ElementId,
    source: SharedString,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
}

impl TextView {
    pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            style: TextViewStyle::default(),
            selectable: false,
            on_link: None,
        }
    }

    pub fn style(mut self, style: TextViewStyle) -> Self {
        self.style = style;
        self
    }

    /// Whether its text can be selected with the pointer and copied.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Called with a link's address when it's pressed (and not dragged across).
    pub fn on_link_click(
        mut self,
        f: impl Fn(&str, &Modifiers, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_link = Some(Rc::new(f));
        self
    }
}

/// A place in the text: a paragraph of the layout and an offset in it.
type Pos = (usize, usize);

/// What a view keeps between draws.
struct ViewState {
    source: SharedString,
    blocks: Rc<Vec<Block>>,
    focus: FocusHandle,
    anchor: Option<Pos>,
    head: Option<Pos>,
    selecting: bool,
    /// A press that hasn't moved yet: a link press if it lets go here.
    press: Option<(Point<Pixels>, Pos)>,
    layout: Option<Rc<ViewLayout>>,
    bounds: Bounds<Pixels>,
}

impl ViewState {
    fn selection(&self) -> Option<(Pos, Pos)> {
        let (a, h) = (self.anchor?, self.head?);
        (a != h).then(|| if a <= h { (a, h) } else { (h, a) })
    }

    fn selected_text(&self) -> Option<String> {
        let ((l0, o0), (l1, o1)) = self.selection()?;
        let layout = self.layout.as_ref()?;
        let mut out = String::new();
        for i in l0..=l1 {
            let leaf = layout.leaves.get(i)?;
            let t = &leaf.paragraph.text;
            let (s, e) = (
                if i == l0 { o0 } else { 0 },
                if i == l1 { o1 } else { t.len() },
            );
            if i > l0 {
                out.push_str(leaf.separator);
            }
            out.push_str(&t[s.min(t.len())..e.min(t.len())]);
        }
        Some(out)
    }
}

/// A paragraph placed in the view.
struct Leaf {
    origin: Point<Pixels>,
    layout: ParagraphLayout,
    paragraph: Rc<Paragraph>,
    /// What goes before it when copied with the one above.
    separator: &'static str,
}

enum Deco {
    Box {
        bounds: Bounds<Pixels>,
        fill: Hsla,
        border: Hsla,
        border_width: Pixels,
        radius: Pixels,
    },
    Text {
        origin: Point<Pixels>,
        layout: ParagraphLayout,
    },
}

struct ViewLayout {
    width: Pixels,
    key: StyleKey,
    height: Pixels,
    leaves: Vec<Leaf>,
    decos: Vec<Deco>,
}

/// Everything layout needs from the theme, the window and the host's style.
struct Ctx<'a> {
    theme: &'a Theme,
    style: &'a TextViewStyle,
    base: ParagraphStyle,
    rem: Pixels,
    gap: Pixels,
}

fn abs(l: AbsoluteLength, rem: Pixels) -> Pixels {
    l.to_pixels(rem)
}

fn def(l: DefiniteLength, rem: Pixels) -> Pixels {
    match l {
        DefiniteLength::Absolute(a) => abs(a, rem),
        DefiniteLength::Fraction(_) => px(0.),
    }
}

impl Ctx<'_> {
    fn heading_size(&self, level: u8) -> Pixels {
        let base = self.style.heading_base_font_size.unwrap_or(self.base.size);
        match &self.style.heading_font_size {
            Some(f) => f(level, base),
            None => {
                base * match level {
                    1 => 1.6,
                    2 => 1.35,
                    3 => 1.15,
                    _ => 1.,
                }
            }
        }
    }

    fn with_size(&self, size: Pixels, bold: bool) -> ParagraphStyle {
        let mut s = self.base.clone();
        s.line_height *= size / s.size;
        s.size = size;
        if bold {
            s.font.weight = FontWeight::SEMIBOLD;
        }
        s
    }

    /// The code block's text style and box, from the host's refinement.
    fn code(&self) -> (ParagraphStyle, Hsla, Hsla, Pixels, Pixels, [Pixels; 4]) {
        let r = &self.style.code_block;
        let mut s = self.base.clone();
        s.font.family = r
            .text
            .font_family
            .clone()
            .unwrap_or_else(|| self.theme.mono_font_family.clone());
        if let Some(size) = r.text.font_size {
            let px_ = size.to_pixels(self.rem);
            s.line_height *= px_ / s.size;
            s.size = px_;
        } else {
            s.size *= 0.9;
        }
        if let Some(lh) = r.text.line_height {
            s.line_height = lh.to_pixels(s.size.into(), self.rem);
        }
        if let Some(c) = r.text.color {
            s.color = c;
        }
        s.code_background = transparent_black();
        let fill_ = match &r.background {
            Some(Fill::Color(bg)) => bg.as_solid().unwrap_or(self.theme.muted),
            _ => self.theme.muted,
        };
        let border = r.border_color.unwrap_or(self.theme.border);
        let bw = r
            .border_widths
            .left
            .map(|w| abs(w, self.rem))
            .unwrap_or(px(0.));
        let radius = r
            .corner_radii
            .top_left
            .map(|w| abs(w, self.rem))
            .unwrap_or(self.theme.radius_md().to_pixels(self.rem));
        let p = &r.padding;
        let pad = [
            p.top.map(|l| def(l, self.rem)).unwrap_or(px(10.)),
            p.right.map(|l| def(l, self.rem)).unwrap_or(px(12.)),
            p.bottom.map(|l| def(l, self.rem)).unwrap_or(px(10.)),
            p.left.map(|l| def(l, self.rem)).unwrap_or(px(12.)),
        ];
        (s, fill_, border, bw, radius, pad)
    }
}

fn lay(
    blocks: &[Block],
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    cx: &Ctx,
    out: &mut ViewLayout,
    window: &mut Window,
) -> Pixels {
    let width = width.max(px(8.));
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            y += cx.gap;
        }
        let sep = if out.leaves.is_empty() { "" } else { "\n\n" };
        match b {
            Block::Paragraph(p) => y = leaf(p, &cx.base, x, width, y, sep, out, window),
            Block::Heading(level, p) => {
                let s = cx.with_size(cx.heading_size(*level), true);
                y = leaf(p, &s, x, width, y, sep, out, window);
            }
            Block::Code { text, .. } => {
                let (s, fill_, border, bw, radius, [pt, pr, pb, pl]) = cx.code();
                let top = y;
                let inner = (width - pl - pr - bw * 2.).max(px(8.));
                let deco_ix = out.decos.len();
                out.decos.push(Deco::Box {
                    bounds: Bounds::default(),
                    fill: fill_,
                    border,
                    border_width: bw,
                    radius,
                });
                y = leaf(
                    &Paragraph::plain(text.clone()),
                    &s,
                    x + pl + bw,
                    inner,
                    y + pt + bw,
                    sep,
                    out,
                    window,
                );
                y += pb + bw;
                out.decos[deco_ix] = Deco::Box {
                    bounds: Bounds::new(point(x, top), size(width, y - top)),
                    fill: fill_,
                    border,
                    border_width: bw,
                    radius,
                };
            }
            Block::Quote(inner) => {
                let top = y;
                let bar = px(3.);
                // An indent never takes more than a quarter of the width it's taken from.
                let indent = (cx.rem * 0.9).min(width * 0.25);
                let deco_ix = out.decos.len();
                out.decos.push(Deco::Box {
                    bounds: Bounds::default(),
                    fill: cx.theme.border,
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: px(1.5),
                });
                y = lay(inner, x + indent, width - indent, y, cx, out, window);
                out.decos[deco_ix] = Deco::Box {
                    bounds: Bounds::new(point(x, top), size(bar, y - top)),
                    fill: cx.theme.border,
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: px(1.5),
                };
            }
            Block::List { start, items } => {
                let indent = (cx.rem * if start.is_some() { 1.6 } else { 1.2 }).min(width * 0.25);
                for (n, item) in items.iter().enumerate() {
                    if n > 0 {
                        y += cx.gap * 0.4;
                    }
                    let marker = match (start, item.checked) {
                        (_, Some(true)) => "☑".to_string(),
                        (_, Some(false)) => "☐".to_string(),
                        (Some(s), None) => format!("{}.", *s as usize + n),
                        (None, None) => "•".to_string(),
                    };
                    let mut ms = cx.base.clone();
                    ms.color = cx.theme.muted_foreground;
                    let m = ParagraphLayout::new(&Paragraph::plain(marker), &ms, indent, window);
                    out.decos.push(Deco::Text {
                        origin: point(x, y),
                        layout: m,
                    });
                    let before = out.leaves.len();
                    y = lay(&item.blocks, x + indent, width - indent, y, cx, out, window);
                    if let Some(l) = out.leaves.get_mut(before)
                        && before > 0
                    {
                        l.separator = "\n";
                    }
                }
            }
            Block::Rule => {
                let mid = y + cx.base.line_height * 0.5;
                out.decos.push(Deco::Box {
                    bounds: Bounds::new(point(x, mid), size(width, px(1.))),
                    fill: cx.theme.border,
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: px(0.),
                });
                y += cx.base.line_height;
            }
            Block::Table { align, rows } => {
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
                let pad = cx.rem * 0.5;
                let col_w = width / cols as f32;
                let top = y;
                for (ri, row) in rows.iter().enumerate() {
                    let row_top = y;
                    let mut row_bottom = y;
                    let s = if ri == 0 {
                        cx.with_size(cx.base.size, true)
                    } else {
                        cx.base.clone()
                    };
                    for c in 0..cols {
                        let p = row.get(c).cloned().unwrap_or_default();
                        let cx0 = x + col_w * c as f32 + pad;
                        let inner = (col_w - pad * 2.).max(px(8.));
                        let sep = if out.leaves.is_empty() {
                            ""
                        } else if c == 0 {
                            "\n"
                        } else {
                            "\t"
                        };
                        // Centred and right-aligned columns lay their text out so.
                        let mut cs = s.clone();
                        cs.align = match align.get(c) {
                            Some(Align::Center) => TextAlign::Center,
                            Some(Align::Right) => TextAlign::Right,
                            _ => TextAlign::Left,
                        };
                        let l = ParagraphLayout::new(&p, &cs, inner, window);
                        let h = l.height;
                        out.leaves.push(Leaf {
                            origin: point(cx0, row_top + pad),
                            layout: l,
                            paragraph: Rc::new(p),
                            separator: sep,
                        });
                        row_bottom = row_bottom.max(row_top + pad * 2. + h);
                    }
                    y = row_bottom;
                    out.decos.push(Deco::Box {
                        bounds: Bounds::new(point(x, y), size(width, px(1.))),
                        fill: cx.theme.border,
                        border: transparent_black(),
                        border_width: px(0.),
                        radius: px(0.),
                    });
                }
                out.decos.push(Deco::Box {
                    bounds: Bounds::new(point(x, top), size(width, y - top)),
                    fill: transparent_black(),
                    border: cx.theme.border,
                    border_width: px(1.),
                    radius: cx.theme.radius_md().to_pixels(cx.rem),
                });
            }
        }
    }
    y
}

#[allow(clippy::too_many_arguments)]
fn leaf(
    p: &Paragraph,
    s: &ParagraphStyle,
    x: Pixels,
    width: Pixels,
    y: Pixels,
    sep: &'static str,
    out: &mut ViewLayout,
    window: &mut Window,
) -> Pixels {
    let l = ParagraphLayout::new(p, s, width, window);
    let h = l.height;
    out.leaves.push(Leaf {
        origin: point(x, y),
        layout: l,
        paragraph: Rc::new(p.clone()),
        separator: sep,
    });
    y + h
}

/// What a layout depends on besides its width: the text's size, colour and family, and the rem.
type StyleKey = (Pixels, Hsla, SharedString, Pixels);

fn style_key(text: &gpui::TextStyle, rem: Pixels) -> StyleKey {
    (
        text.font_size.to_pixels(rem),
        text.color,
        text.font_family.clone(),
        rem,
    )
}

/// Lays the view out at `width` in `text`, the text style around it. That style is taken where
/// GPUI applies its parents' (while laying out or painting), not inside a measure, which runs
/// outside them.
fn layout_for(
    state: &ViewState,
    style: &TextViewStyle,
    text: &gpui::TextStyle,
    width: Pixels,
    window: &mut Window,
    cx: &App,
) -> ViewLayout {
    let theme = cx.theme();
    let rem = window.rem_size();
    let size = text.font_size.to_pixels(rem);
    let base = ParagraphStyle {
        font: text.font(),
        size,
        line_height: text.line_height_in_pixels(rem),
        color: text.color,
        muted: theme.muted_foreground,
        link: theme.foreground,
        code_family: theme.mono_font_family.clone(),
        code_background: theme.muted,
        align: TextAlign::Left,
    };
    let c = Ctx {
        theme,
        style,
        base,
        rem,
        gap: style.paragraph_gap.to_pixels(rem),
    };
    let mut out = ViewLayout {
        width,
        key: style_key(text, rem),
        height: px(0.),
        leaves: Vec::new(),
        decos: Vec::new(),
    };
    out.height = lay(&state.blocks, px(0.), width, px(0.), &c, &mut out, window);
    out
}

impl RenderOnce for TextView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ViewState {
            source: SharedString::default(),
            blocks: Rc::new(Vec::new()),
            focus: cx.focus_handle(),
            anchor: None,
            head: None,
            selecting: false,
            press: None,
            layout: None,
            bounds: Bounds::default(),
        });
        state.update(cx, |s, _| {
            if s.source != self.source {
                s.blocks = Rc::new(parse(&self.source));
                s.source = self.source.clone();
                s.layout = None;
                s.anchor = None;
                s.head = None;
            }
        });
        let focus = state.read(cx).focus.clone();
        let plain: SharedString = self.source.clone();
        let st = state.clone();
        let st2 = state.clone();
        let st3 = state.clone();
        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .track_focus(&focus)
            .role(Role::Document)
            .aria_label(plain)
            .w_full()
            .min_w_0()
            .on_action(move |_: &CopySelection, _, cx| {
                if let Some(t) = st.read(cx).selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(t));
                }
            })
            .on_action(move |_: &SelectAllText, _, cx| {
                st2.update(cx, |s, cx| {
                    if let Some(l) = &s.layout
                        && let Some(last) = l.leaves.len().checked_sub(1)
                    {
                        s.anchor = Some((0, 0));
                        s.head = Some((last, l.leaves[last].paragraph.text.len()));
                        cx.notify();
                    }
                })
            })
            .on_action(move |_: &ClearSelection, _, cx| {
                st3.update(cx, |s, cx| {
                    s.anchor = None;
                    s.head = None;
                    cx.notify();
                })
            })
            .child(ViewElement {
                state,
                style: self.style,
                selectable: self.selectable,
                on_link: self.on_link,
            })
    }
}

/// A draw's layout, and the hitboxes its cursors go on.
struct Prepainted {
    layout: Rc<ViewLayout>,
    body: Option<gpui::Hitbox>,
    links: Vec<gpui::Hitbox>,
}

struct ViewElement {
    state: gpui::Entity<ViewState>,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
}

impl IntoElement for ViewElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl ViewLayout {
    /// The place nearest `p`, from the view's top left.
    fn pos_at(&self, p: Point<Pixels>) -> Option<Pos> {
        let mut best: Option<(Pixels, usize)> = None;
        for (i, l) in self.leaves.iter().enumerate() {
            let top = l.origin.y;
            let bottom = top + l.layout.height;
            let dy = if p.y < top {
                top - p.y
            } else if p.y > bottom {
                p.y - bottom
            } else {
                px(0.)
            };
            let left = l.origin.x;
            let right = left + l.layout.width;
            let dx = if p.x < left {
                left - p.x
            } else if p.x > right {
                p.x - right
            } else {
                px(0.)
            };
            let d = dy * 4. + dx;
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        let (_, i) = best?;
        let l = &self.leaves[i];
        Some((i, l.layout.index_for(p - l.origin)))
    }

    fn word_at(&self, (leaf, at): Pos) -> (Pos, Pos) {
        let t = &self.leaves[leaf].paragraph.text;
        for (i, w) in t.split_word_bound_indices() {
            if at >= i && at < i + w.len() {
                return ((leaf, i), (leaf, i + w.len()));
            }
        }
        ((leaf, at), (leaf, at))
    }
}

impl Element for ViewElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.min_size.width = Length::Definite(px(0.).into());
        let state = self.state.clone();
        let tv_style = self.style.clone();
        let text_style = window.text_style();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let width = width.unwrap_or(px(400.));
            let key = style_key(&text_style, window.rem_size());
            let cached = state
                .read(cx)
                .layout
                .clone()
                .filter(|l| l.width == width && l.key == key);
            let l = match cached {
                Some(l) => l,
                None => {
                    let l = Rc::new(layout_for(
                        state.read(cx),
                        &tv_style,
                        &text_style,
                        width,
                        window,
                        cx,
                    ));
                    state.update(cx, |s, _| s.layout = Some(l.clone()));
                    l
                }
            };
            size(width, l.height)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepainted {
        let width = bounds.size.width;
        let text_style = window.text_style();
        let key = style_key(&text_style, window.rem_size());
        let cached = self
            .state
            .read(cx)
            .layout
            .clone()
            .filter(|l| l.width == width && l.key == key);
        let l = match cached {
            Some(l) => l,
            None => Rc::new(layout_for(
                self.state.read(cx),
                &self.style,
                &text_style,
                width,
                window,
                cx,
            )),
        };
        self.state.update(cx, |s, _| {
            s.layout = Some(l.clone());
            s.bounds = bounds;
        });
        // The text takes an I-beam; links, above it, a pointing hand.
        let body = self
            .selectable
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal));
        let mut links = Vec::new();
        for leaf in &l.leaves {
            for (r, m) in &leaf.paragraph.spans {
                if m.link.is_some() {
                    for b in leaf.layout.bands(r.clone()) {
                        links.push(window.insert_hitbox(
                            Bounds::new(bounds.origin + leaf.origin + b.origin, b.size),
                            HitboxBehavior::Normal,
                        ));
                    }
                }
            }
        }
        Prepainted {
            layout: l,
            body,
            links,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepainted: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let l = &prepainted.layout;
        let theme = cx.theme().clone();
        let o = bounds.origin;
        let (focused, selection) = {
            let s = self.state.read(cx);
            (s.focus.is_focused(window), s.selection())
        };
        for d in &l.decos {
            if let Deco::Box {
                bounds: b,
                fill: f,
                border,
                border_width,
                radius,
            } = d
            {
                window.paint_quad(quad(
                    Bounds::new(o + b.origin, b.size),
                    *radius,
                    *f,
                    *border_width,
                    *border,
                    Default::default(),
                ));
            }
        }
        if focused && let Some(((l0, o0), (l1, o1))) = selection {
            for (i, leaf) in l.leaves.iter().enumerate().take(l1 + 1).skip(l0) {
                let len = leaf.paragraph.text.len();
                let r = (if i == l0 { o0 } else { 0 })..(if i == l1 { o1 } else { len });
                for b in leaf.layout.bands(r) {
                    window.paint_quad(fill(
                        Bounds::new(o + leaf.origin + b.origin, b.size),
                        theme.selection(),
                    ));
                }
            }
        }
        for d in &l.decos {
            if let Deco::Text { origin, layout } = d {
                layout.paint(o + *origin, window, cx);
            }
        }
        // Each paragraph is clipped to the width it was laid out in, with three pixels' room
        // either side for ink past a glyph's advance: a character wider than a narrow table
        // cell is cut there rather than drawn over its neighbour.
        for leaf in &l.leaves {
            // Ink reaches further past the advance the bigger the text (a large emoji's most),
            // so the room is a share of the paragraph's line.
            let ink = (window.rem_size() * 0.2).max(leaf.layout.line_height * 0.3);
            let clip = Bounds::new(
                o + leaf.origin - point(ink, px(0.)),
                size(leaf.layout.width + ink * 2., leaf.layout.height),
            );
            window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
                leaf.layout.paint(o + leaf.origin, window, cx)
            });
        }

        if let Some(hb) = &prepainted.body {
            window.set_cursor_style(CursorStyle::IBeam, hb);
        }
        for hb in &prepainted.links {
            window.set_cursor_style(CursorStyle::PointingHand, hb);
        }

        let layout = l.clone();
        let state = self.state.clone();
        let selectable = self.selectable;
        let on_link = self.on_link.clone();
        window.on_mouse_event({
            let layout = layout.clone();
            let state = state.clone();
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || e.button != MouseButton::Left
                    || !bounds.contains(&e.position)
                {
                    return;
                }
                let Some(pos) = layout.pos_at(e.position - bounds.origin) else {
                    return;
                };
                let focus = state.read(cx).focus.clone();
                if selectable {
                    window.focus(&focus, cx);
                }
                state.update(cx, |s, cx| {
                    s.press = Some((e.position, pos));
                    if selectable {
                        let (a, h) = match e.click_count {
                            2 => layout.word_at(pos),
                            n if n >= 3 => (
                                (pos.0, 0),
                                (pos.0, layout.leaves[pos.0].paragraph.text.len()),
                            ),
                            _ if e.modifiers.shift && s.anchor.is_some() => {
                                (s.anchor.unwrap_or(pos), pos)
                            }
                            _ => (pos, pos),
                        };
                        s.anchor = Some(a);
                        s.head = Some(h);
                        s.selecting = true;
                    }
                    cx.notify();
                });
            }
        });
        if state.read(cx).selecting || state.read(cx).press.is_some() {
            window.on_mouse_event({
                let layout = layout.clone();
                let state = state.clone();
                move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble || e.pressed_button != Some(MouseButton::Left)
                    {
                        return;
                    }
                    state.update(cx, |s, cx| {
                        if let Some((at, _)) = s.press
                            && (e.position - at).magnitude() > 3.
                        {
                            s.press = None;
                        }
                        if s.selecting
                            && let Some(pos) = layout.pos_at(e.position - bounds.origin)
                        {
                            s.head = Some(pos);
                            cx.notify();
                        }
                    });
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let press = state.update(cx, |s, cx| {
                    s.selecting = false;
                    cx.notify();
                    s.press.take()
                });
                // A press that didn't move, on a link: the host's to open.
                if let (Some((_, (leaf, at))), Some(f)) = (press, &on_link)
                    && let Some(url) = layout
                        .leaves
                        .get(leaf)
                        .and_then(|l| l.paragraph.link_at(at))
                        .cloned()
                {
                    let collapsed = state.read(cx).selection().is_none();
                    if collapsed || !selectable {
                        f(&url, &e.modifiers, window, cx);
                    }
                }
            });
        }
    }
}
