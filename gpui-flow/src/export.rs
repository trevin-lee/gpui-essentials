//! A flow as a picture: SVG, and PNG made from it.
//!
//! The SVG is one standalone image of every node and edge, as the canvas draws them: shapes
//! with their outlines, frames, headings, link cards, pictures (as their frame and alt text:
//! the image never points at another file), and edges along the routes they're drawn on, with
//! their dashes, arrowheads and labels. Text is wrapped to each node and clipped to it, and
//! everything taken from the flow is escaped, so whatever a node says, the file is only ever a
//! picture.

use std::fmt::Write as _;

use gpui::{App, Hsla, Rgba};

use crate::flow::Flow;
use crate::geometry::edge_path;
use crate::shapes::{PathOp, geometry};
use crate::theme::FlowColors;
use crate::types::*;

/// How far round the flow the picture reaches, in flow units.
const MARGIN: f32 = 24.;

impl Flow {
    /// Every node and edge as one standalone SVG image, in the flow's colors (the theme's
    /// unless it has its own). Nodes the canvas hasn't measured yet take their given size, or
    /// a default.
    pub fn to_svg(&self, cx: &App) -> String {
        let colors = self.colors(cx);
        svg(self, &colors)
    }
}

fn svg(flow: &Flow, colors: &FlowColors) -> String {
    let nodes: Vec<(&Node, Rect)> = flow
        .nodes()
        .iter()
        .filter(|n| flow.shown(n))
        .map(|n| {
            (
                n,
                flow.node_rect(&n.id)
                    .unwrap_or_else(|| fallback_rect(flow, n)),
            )
        })
        .collect();
    let bounds = nodes
        .iter()
        .map(|(_, r)| *r)
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default();
    let area = Rect::new(
        bounds.x - MARGIN,
        bounds.y - MARGIN,
        bounds.w + MARGIN * 2.,
        bounds.h + MARGIN * 2.,
    );
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{x} {y} {w} {h}" font-family="-apple-system, 'Helvetica Neue', Helvetica, Arial, sans-serif">"#,
        x = n(area.x),
        y = n(area.y),
        w = n(area.w),
        h = n(area.h),
    );
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
        n(area.x),
        n(area.y),
        n(area.w),
        n(area.h),
        hex(colors.background)
    );
    // Frames and groups first, then edges, then the rest, as the canvas layers them.
    let under = |n: &Node| matches!(n.kind.as_ref(), "frame" | "group");
    for (i, (node, rect)) in nodes.iter().enumerate().filter(|(_, (n, _))| under(n)) {
        draw_node(&mut out, i, node, *rect, colors);
    }
    for edge in flow.edges().iter().filter(|e| !e.hidden) {
        draw_edge(&mut out, flow, edge, colors);
    }
    for (i, (node, rect)) in nodes.iter().enumerate().filter(|(_, (n, _))| !under(n)) {
        draw_node(&mut out, i, node, *rect, colors);
    }
    out.push_str("</svg>");
    out
}

/// Where a node is when the canvas hasn't measured it: at its place, at its given size or a
/// default one.
fn fallback_rect(flow: &Flow, node: &Node) -> Rect {
    let at = flow.absolute_position(&node.id).unwrap_or(node.position);
    let (dw, dh) = match node.kind.as_ref() {
        "shape" => node.shape.unwrap_or_default().default_size(),
        "frame" => (360., 240.),
        _ => (150., 40.),
    };
    Rect::new(
        at.x,
        at.y,
        node.width.unwrap_or(dw),
        node.height.unwrap_or(dh),
    )
}

fn draw_node(out: &mut String, i: usize, node: &Node, r: Rect, colors: &FlowColors) {
    let pencil = colors.pencil(node.color.as_deref());
    let clip = format!("n{i}");
    // Everything a node draws stays in its box.
    let _ = write!(
        out,
        r#"<clipPath id="{clip}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#{clip})">"#,
        n(r.x - 1.),
        n(r.y - 1.),
        n(r.w + 2.),
        n(r.h + 2.)
    );
    let label = node.label_text();
    match node.kind.as_ref() {
        "shape" => {
            let shape = node.shape.unwrap_or_default();
            let g = geometry(shape, r.w, r.h);
            let _ = write!(
                out,
                r#"<path d="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
                path(&g.outline, r.x, r.y),
                hex(pencil.fill),
                hex(pencil.stroke)
            );
            if !g.detail.is_empty() {
                let _ = write!(
                    out,
                    r#"<path d="{}" fill="none" stroke="{}" stroke-width="1"/>"#,
                    path(&g.detail, r.x, r.y),
                    hex(pencil.stroke)
                );
            }
            let t = g.text;
            text_block(
                out,
                &label,
                Rect::new(r.x + t.x, r.y + t.y, t.w, t.h),
                13.,
                pencil.text,
                node.text_align.unwrap_or_default(),
                true,
            );
        }
        "frame" => {
            let title = 28.;
            let fill = if node.color.is_some() {
                pencil.fill
            } else {
                colors.group
            };
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="{}" stroke="{}"/>"#,
                n(r.x),
                n(r.y + title),
                n(r.w),
                n((r.h - title).max(0.)),
                hex(fill),
                hex(if node.color.is_some() {
                    pencil.stroke
                } else {
                    colors.node_border
                })
            );
            text_block(
                out,
                &label,
                Rect::new(r.x + 4., r.y, r.w - 8., title),
                12.,
                if node.color.is_some() {
                    pencil.text
                } else {
                    colors.muted_text
                },
                LabelAlign::Left,
                true,
            );
        }
        "group" => {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="{}" stroke="{}"/>"#,
                n(r.x),
                n(r.y),
                n(r.w),
                n(r.h),
                hex(colors.group),
                hex(colors.node_border)
            );
            text_block(
                out,
                &label,
                inset(r, 6.),
                10.,
                colors.muted_text,
                LabelAlign::Left,
                false,
            );
        }
        "text" => {
            let color = if node.color.is_some() {
                pencil.text
            } else {
                colors.node_text
            };
            text_block(
                out,
                &label,
                inset(r, 4.),
                16.,
                color,
                node.text_align.unwrap_or(LabelAlign::Left),
                false,
            );
        }
        "drawing" => {
            let points = node
                .data
                .get("points")
                .and_then(|p| p.as_str())
                .map(crate::node::parse_points)
                .unwrap_or_default();
            if points.len() >= 2 {
                let d: String = points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        format!(
                            "{}{} {}",
                            if i == 0 { "M" } else { "L" },
                            n(r.x + p.x * r.w),
                            n(r.y + p.y * r.h)
                        )
                    })
                    .collect();
                let _ = write!(
                    out,
                    r#"<path d="{d}" fill="none" stroke="{}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>"#,
                    hex(if node.color.is_some() {
                        pencil.stroke
                    } else {
                        colors.node_text
                    })
                );
            }
        }
        "image" => {
            // The picture is another file, which a standalone image can't show: its frame and
            // what it shows instead.
            box_rect(out, r, 6., colors.group, colors.node_border);
            let alt = node
                .media
                .as_ref()
                .map(|m| m.alt.clone())
                .unwrap_or_default();
            text_block(
                out,
                &alt,
                inset(r, 10.),
                11.,
                colors.muted_text,
                LabelAlign::Center,
                true,
            );
        }
        "link" => {
            box_rect(out, r, 6., colors.node, colors.node_border);
            let card = node.card.clone().unwrap_or_default();
            let mut lines = Vec::new();
            if let Some(site) = &card.site {
                lines.push(site.to_string());
            }
            lines.push(
                card.title
                    .clone()
                    .unwrap_or_else(|| card.url.clone())
                    .to_string(),
            );
            if let Some(d) = &card.description {
                lines.push(d.to_string());
            }
            lines.push(card.url.to_string());
            text_block(
                out,
                &lines.join("\n"),
                inset(r, 12.),
                11.,
                colors.node_text,
                LabelAlign::Left,
                false,
            );
        }
        _ => {
            // The built-in boxes, and kinds of the host's own, as a box with the label.
            let radius = if node.kind == "topic" { 7. } else { 3. };
            box_rect(out, r, radius, pencil.fill, pencil.stroke);
            text_block(
                out,
                &label,
                inset(r, 10.),
                12.,
                pencil.text,
                node.text_align.unwrap_or_default(),
                true,
            );
        }
    }
    out.push_str("</g>");
}

fn inset(r: Rect, by: f32) -> Rect {
    Rect::new(
        r.x + by,
        r.y + by,
        (r.w - by * 2.).max(0.),
        (r.h - by * 2.).max(0.),
    )
}

fn box_rect(out: &mut String, r: Rect, radius: f32, fill: Hsla, stroke: Hsla) {
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{radius}" fill="{}" stroke="{}"/>"#,
        n(r.x + 0.5),
        n(r.y + 0.5),
        n((r.w - 1.).max(0.)),
        n((r.h - 1.).max(0.)),
        hex(fill),
        hex(stroke)
    );
}

/// Text wrapped to a box at about the width its letters take, line by line, centred in the box
/// up and down when `middle`. What doesn't fit is clipped with the node.
fn text_block(
    out: &mut String,
    text: &str,
    area: Rect,
    size: f32,
    color: Hsla,
    align: LabelAlign,
    middle: bool,
) {
    if text.trim().is_empty() || area.w <= 0. {
        return;
    }
    let lines = wrap(text, area.w, size);
    let line_height = size * 1.35;
    let height = line_height * lines.len() as f32;
    let top = if middle {
        area.y + (area.h - height).max(0.) / 2.
    } else {
        area.y
    };
    let (x, anchor) = match align {
        LabelAlign::Left => (area.x, "start"),
        LabelAlign::Center => (area.x + area.w / 2., "middle"),
        LabelAlign::Right => (area.right(), "end"),
    };
    let _ = write!(
        out,
        r#"<text font-size="{}" fill="{}" text-anchor="{anchor}">"#,
        n(size),
        hex(color)
    );
    for (i, line) in lines.iter().enumerate() {
        let _ = write!(
            out,
            r#"<tspan x="{}" y="{}">{}</tspan>"#,
            n(x),
            n(top + line_height * i as f32 + size),
            escape(line)
        );
    }
    out.push_str("</text>");
}

/// Lines no wider than `width`, reckoning a letter at a little over half the size (a wide one
/// at the whole size); a word too long for a line breaks.
fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    let advance = |c: char| {
        if c.is_ascii() { size * 0.56 } else { size }
    };
    let mut lines = Vec::new();
    for paragraph in text.split('\n').take(200) {
        let mut line = String::new();
        let mut used = 0.;
        for word in paragraph.split(' ') {
            let w: f32 = word.chars().map(advance).sum();
            let space = if line.is_empty() { 0. } else { advance(' ') };
            if used + space + w <= width {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                used += space + w;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0.;
            }
            // A word too long for a line breaks wherever the line is full.
            for c in word.chars() {
                if used + advance(c) > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    used = 0.;
                }
                line.push(c);
                used += advance(c);
            }
        }
        lines.push(line);
        if lines.len() > 400 {
            break;
        }
    }
    lines
}

fn draw_edge(out: &mut String, flow: &Flow, edge: &Edge, colors: &FlowColors) {
    let Some(points) = flow
        .edge_line(&edge.id)
        .or_else(|| computed_line(flow, edge))
    else {
        return;
    };
    if points.len() < 2 {
        return;
    }
    let color = edge
        .color
        .map(|c| gpui::rgb(c).into())
        .or_else(|| edge.pencil.as_deref().and_then(|k| colors.palette.get(k)))
        .unwrap_or(colors.edge);
    let width = edge.stroke_width.unwrap_or(1.);
    let d: String = points
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}{} {}", if i == 0 { "M" } else { "L" }, n(p.x), n(p.y)))
        .collect();
    let dash = match edge.dash {
        EdgeDash::Solid if edge.animated => r#" stroke-dasharray="5 5""#.to_string(),
        EdgeDash::Solid => String::new(),
        EdgeDash::Dashed => r#" stroke-dasharray="6 4""#.to_string(),
        EdgeDash::Dotted => r#" stroke-dasharray="1.5 4""#.to_string(),
    };
    let _ = write!(
        out,
        r#"<path d="{d}" fill="none" stroke="{}" stroke-width="{}"{dash}/>"#,
        hex(color),
        n(width)
    );
    let last = points.len() - 1;
    if let Some(m) = &edge.marker_end {
        marker(out, m, points[last], points[last - 1], width, color);
    }
    if let Some(m) = &edge.marker_start {
        marker(out, m, points[0], points[1], width, color);
    }
    if let (Some(label), Some(at)) = (
        edge.label.as_ref().filter(|l| !l.trim().is_empty()),
        flow.edge_label_point(&edge.id)
            .or_else(|| Some(points[points.len() / 2])),
    ) {
        let lines = wrap(label, crate::paint::LABEL_WIDTH, 10.);
        let lines: Vec<String> = lines.into_iter().take(4).collect();
        let w = lines
            .iter()
            .map(|l| l.chars().count() as f32 * 10. * 0.56)
            .fold(0., f32::max)
            .min(crate::paint::LABEL_WIDTH);
        let h = lines.len() as f32 * 13.5;
        let area = Rect::new(at.x - w / 2. - 5., at.y - h / 2. - 2., w + 10., h + 4.);
        box_rect(
            out,
            area,
            4.,
            colors.edge_label_background,
            colors.edge_label_background,
        );
        text_block(
            out,
            &lines.join("\n"),
            inset(area, 2.),
            10.,
            colors.edge_label,
            LabelAlign::Center,
            true,
        );
    }
}

/// An edge's line when the canvas hasn't drawn it: between its handles, or else the facing
/// sides of its nodes.
fn computed_line(flow: &Flow, edge: &Edge) -> Option<Vec<XY>> {
    let ends = |id: &NodeId, kind: HandleKind, handle: Option<&str>, other: &NodeId| {
        if let Some(p) = flow.handle_point(id, kind, handle) {
            return Some(p);
        }
        let (a, b) = (flow.node_rect(id)?, flow.node_rect(other)?);
        let d = b.center() - a.center();
        Some(if d.x.abs() >= d.y.abs() {
            xy(if d.x >= 0. { a.right() } else { a.x }, a.center().y)
        } else {
            xy(a.center().x, if d.y >= 0. { a.bottom() } else { a.y })
        })
    };
    let from = ends(
        &edge.source,
        HandleKind::Source,
        edge.source_handle.as_deref(),
        &edge.target,
    )?;
    let to = ends(
        &edge.target,
        HandleKind::Target,
        edge.target_handle.as_deref(),
        &edge.source,
    )?;
    let side = |a: XY, b: XY| {
        let d = b - a;
        if d.x.abs() >= d.y.abs() {
            if d.x >= 0. { Side::Right } else { Side::Left }
        } else if d.y >= 0. {
            Side::Bottom
        } else {
            Side::Top
        }
    };
    Some(edge_path(&edge.kind, from, side(from, to), to, side(to, from)).flatten(4.))
}

fn marker(out: &mut String, m: &Marker, tip: XY, from: XY, width: f32, color: Hsla) {
    let length = from.distance(tip);
    if length <= 0. {
        return;
    }
    let dir = (tip - from) * (1. / length);
    let normal = xy(-dir.y, dir.x);
    let base = tip - dir * m.size;
    let (a, b) = (
        base + normal * (m.size * 0.5),
        base - normal * (m.size * 0.5),
    );
    let color = m.color.map(|c| gpui::rgb(c).into()).unwrap_or(color);
    match m.kind {
        MarkerKind::ArrowClosed => {
            let _ = write!(
                out,
                r#"<path d="M{} {}L{} {}L{} {}Z" fill="{}"/>"#,
                n(a.x),
                n(a.y),
                n(tip.x),
                n(tip.y),
                n(b.x),
                n(b.y),
                hex(color)
            );
        }
        MarkerKind::Arrow => {
            let _ = write!(
                out,
                r#"<path d="M{} {}L{} {}L{} {}" fill="none" stroke="{}" stroke-width="{}"/>"#,
                n(a.x),
                n(a.y),
                n(tip.x),
                n(tip.y),
                n(b.x),
                n(b.y),
                hex(color),
                n(width)
            );
        }
    }
}

fn path(ops: &[PathOp], dx: f32, dy: f32) -> String {
    let mut d = String::new();
    for op in ops {
        let _ = match *op {
            PathOp::Move(p) => write!(d, "M{} {}", n(p.x + dx), n(p.y + dy)),
            PathOp::Line(p) => write!(d, "L{} {}", n(p.x + dx), n(p.y + dy)),
            PathOp::Cubic { c1, c2, to } => write!(
                d,
                "C{} {} {} {} {} {}",
                n(c1.x + dx),
                n(c1.y + dy),
                n(c2.x + dx),
                n(c2.y + dy),
                n(to.x + dx),
                n(to.y + dy)
            ),
            PathOp::Close => write!(d, "Z"),
        };
    }
    d
}

/// A number as SVG wants it: two decimals at most, and never NaN.
fn n(v: f32) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn hex(c: Hsla) -> String {
    let c: Rgba = c.into();
    let to = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    if c.a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", to(c.r), to(c.g), to(c.b))
    } else {
        format!(
            "rgba({},{},{},{:.3})",
            to(c.r),
            to(c.g),
            to(c.b),
            c.a.clamp(0., 1.)
        )
    }
}

/// Text made safe for SVG: markup characters escaped, and characters XML can't hold dropped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Why a picture couldn't be made.
#[cfg(feature = "png")]
#[derive(Debug)]
pub struct ExportError(pub String);

#[cfg(feature = "png")]
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "png")]
impl std::error::Error for ExportError {}

/// An SVG (as [`Flow::to_svg`] makes) as a PNG, `scale` pixels to a point (2 for a Retina
/// screen), with the system's fonts. At most 16384 pixels a side.
#[cfg(feature = "png")]
pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, ExportError> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options)
        .map_err(|e| ExportError(format!("not an SVG: {e}")))?;
    let size = tree.size();
    let scale = if scale.is_finite() && scale > 0. {
        scale
    } else {
        1.
    };
    let max = 16384.;
    let scale = scale.min(max / size.width()).min(max / size.height());
    let (w, h) = (
        (size.width() * scale).ceil().max(1.) as u32,
        (size.height() * scale).ceil().max(1.) as u32,
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| ExportError("too large a picture".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| ExportError(format!("couldn't write the PNG: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_wraps_to_its_width_and_breaks_long_words() {
        let lines = wrap("a short line and then Pneumonoultramicroscopic", 60., 10.);
        for l in &lines {
            let w: f32 = l.chars().map(|_| 5.6).sum();
            assert!(w <= 60. + 5.6, "{l:?}");
        }
        assert!(lines.len() >= 3);
    }

    #[test]
    fn markup_is_escaped() {
        assert_eq!(
            escape("<script>alert('x')</script> & \"\u{0}"),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;"
        );
    }

    #[test]
    fn numbers_are_short_and_finite() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(-0.001), "0");
        assert_eq!(n(2.345), "2.35");
        assert_eq!(n(f32::NAN), "0");
    }
}
