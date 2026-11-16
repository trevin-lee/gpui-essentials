//! The flow's colors. By default they come from the gpui-core theme, so the flow looks like the
//! app around it; a flow can be given its own.

use gpui::{App, Hsla, rgb};
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
        }
    }
}
