//! The flow's colors. By default they come from the gpui-core theme, so the flow looks like the
//! app around it; a flow can be given its own.

use gpui::{App, Hsla, Rgba, rgb};
use gpui_core::Theme;

/// Every color the flow draws with.
#[derive(Clone, Copy, Debug)]
pub struct FlowColors {
    /// The pane behind everything.
    pub background: Hsla,
    /// The dots, lines or crosses on the pane.
    pub pattern: Hsla,
    pub node: Hsla,
    pub node_border: Hsla,
    pub node_text: Hsla,
    pub muted_text: Hsla,
    /// A group node's fill.
    pub group: Hsla,
    /// The border of what's selected, the selected edges, and a connection about to be made.
    pub selected: Hsla,
    /// The ring round the node or edge the keyboard is on.
    pub focus: Hsla,
    pub edge: Hsla,
    pub edge_label: Hsla,
    pub edge_label_background: Hsla,
    pub handle: Hsla,
    pub handle_border: Hsla,
    /// A connection that can't be made where it's being dropped.
    pub invalid: Hsla,
    pub selection_fill: Hsla,
    /// The controls and the minimap.
    pub panel: Hsla,
    pub panel_border: Hsla,
    pub panel_hover: Hsla,
    pub panel_icon: Hsla,
    pub minimap_node: Hsla,
    /// Laid over the part of the minimap the pane isn't showing.
    pub minimap_mask: Hsla,
    /// The outline of the part the pane is showing.
    pub minimap_view: Hsla,
    /// The lines that show what a dragged node lines up with.
    pub guide: Hsla,
    /// The colors a node or an edge can be given by name (`Node::color`), as strokes.
    pub palette: Palette,
}

/// The named colors, each a pencil's stroke: a node's fill is the same pencil pressed lightly
/// on the pane, and its text a shade of it that reads on that fill. Give a flow the host
/// theme's own with [`FlowColors::palette`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub red: Hsla,
    pub orange: Hsla,
    pub yellow: Hsla,
    pub green: Hsla,
    pub teal: Hsla,
    pub blue: Hsla,
    pub purple: Hsla,
    pub pink: Hsla,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            red: rgb(0xe5534b).into(),
            orange: rgb(0xf0883e).into(),
            yellow: rgb(0xd4a72c).into(),
            green: rgb(0x3fb950).into(),
            teal: rgb(0x2dbaa6).into(),
            blue: rgb(0x4f8ff7).into(),
            purple: rgb(0xa371f7).into(),
            pink: rgb(0xdb61a2).into(),
        }
    }
}

impl Palette {
    /// The names a color can be given by, besides `default`.
    pub const KEYS: [&'static str; 8] = [
        "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
    ];

    /// The stroke named `key`, if it's one of [`Palette::KEYS`].
    pub fn get(&self, key: &str) -> Option<Hsla> {
        Some(match key {
            "red" => self.red,
            "orange" => self.orange,
            "yellow" => self.yellow,
            "green" => self.green,
            "teal" => self.teal,
            "blue" => self.blue,
            "purple" => self.purple,
            "pink" => self.pink,
            _ => return None,
        })
    }
}

/// How a node with a named color is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pencil {
    pub stroke: Hsla,
    pub fill: Hsla,
    pub text: Hsla,
}

/// `a` mixed with `b`, `t` of the way to `b`, opaque.
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
    .into()
}

impl FlowColors {
    /// The flow's colors for the current gpui-core theme.
    pub fn from_theme(cx: &App) -> Self {
        let c = &Theme::global(cx).colors;
        let selected: Hsla = rgb(0x4f8ff7).into();
        Self {
            background: c.background,
            pattern: c.muted_foreground.opacity(0.35),
            node: c.popover,
            node_border: c.input,
            node_text: c.foreground,
            muted_text: c.muted_foreground,
            group: c.muted_foreground.opacity(0.08),
            selected,
            focus: c.ring,
            edge: c.muted_foreground,
            edge_label: c.foreground,
            edge_label_background: c.background,
            handle: c.foreground,
            handle_border: c.background,
            invalid: c.destructive,
            selection_fill: selected.opacity(0.08),
            panel: c.popover,
            panel_border: c.border,
            panel_hover: c.muted,
            panel_icon: c.foreground,
            minimap_node: c.muted_foreground.opacity(0.5),
            minimap_mask: c.background.opacity(0.7),
            minimap_view: c.muted_foreground.opacity(0.6),
            guide: rgb(0xdb61a2).into(),
            palette: Palette::default(),
        }
    }

    /// The ink colour of the palette's colour named `key`, as `0xRRGGBBAA`: black for none,
    /// `default` or a name the palette doesn't have (black is drawn light on a dark canvas).
    pub fn ink(&self, key: Option<&str>) -> u32 {
        let Some(c) = key.and_then(|k| self.palette.get(k)) else {
            return 0x0000_00ff;
        };
        let c = c.to_rgb();
        let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
        byte(c.r) << 24 | byte(c.g) << 16 | byte(c.b) << 8 | 0xff
    }

    /// How a node of the color named `key` is drawn: the plain node colors for `default` or
    /// a name the palette doesn't have.
    pub fn pencil(&self, key: Option<&str>) -> Pencil {
        let Some(stroke) = key.and_then(|k| self.palette.get(k)) else {
            return Pencil {
                stroke: self.node_border,
                fill: self.node,
                text: self.node_text,
            };
        };
        // The fill is the pencil pressed lightly onto the pane (mixed, not see-through, so the
        // pattern never shows through), and the text a shade of it that reads on that fill.
        let dark = self.background.l < 0.5;
        let text = Hsla {
            l: if dark {
                stroke.l.max(0.75)
            } else {
                stroke.l.min(0.32)
            },
            ..stroke
        };
        Pencil {
            stroke,
            fill: mix(self.background, stroke, if dark { 0.16 } else { 0.1 }),
            text,
        }
    }
}
