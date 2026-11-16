//! The containers toggles and callouts are drawn in.
//!
//! The editor projects a toggle and a callout as a gpui-kit block surface
//! (`BlockNode::Surface`), whose container the host registers by name on the
//! editor's `MarkdownExtensions`. [`with_default_surfaces`] registers a plain
//! set: a chevron gutter for toggles, and a tinted box for GitHub's five alert
//! names. Register the same names again afterwards to draw them otherwise; a
//! later registration replaces an earlier one.

use gpui::{App, Div, Hsla, ParentElement as _, Styled as _, Window, div, px, rems};
use gpui_kit::base::text::MarkdownExtensions;
use gpui_kit::component::{ActiveTheme as _, Icon};

use crate::direct::{TOGGLE_CLOSED_SURFACE, TOGGLE_OPEN_SURFACE};

// Lucide icons (ISC, see assets/icons/LICENSE-LUCIDE).
const ICON_CHEVRON_RIGHT: &[u8] = include_bytes!("../assets/icons/chevron-right.svg");
const ICON_CHEVRON_DOWN: &[u8] = include_bytes!("../assets/icons/chevron-down.svg");

/// Width of a toggle's gutter, where its chevron is drawn and clicked, in
/// rems of the editor's text.
pub const TOGGLE_GUTTER_REMS: f32 = 1.5;

/// The callout names GitHub draws (`> [!NOTE]` and the rest).
pub const ALERT_NAMES: [&str; 5] = ["note", "tip", "important", "warning", "caution"];

/// A toggle's container: the summary and body indented past a gutter that
/// shows the chevron level with the summary's first line. The chevron is not
/// document text; a click in the gutter opens or closes the toggle (the
/// editor hit-tests the gutter, so this container needs no handler).
pub fn toggle_container(open: bool, window: &mut Window, cx: &mut App) -> Div {
    let line = window.line_height();
    let icon = rems(1.).to_pixels(window.rem_size());
    let top = ((line - icon) / 2.).max(px(0.));
    div().relative().pl(rems(TOGGLE_GUTTER_REMS)).child(
        div().absolute().left(px(0.)).top(top).child(
            Icon::default()
                .data(if open {
                    ICON_CHEVRON_DOWN
                } else {
                    ICON_CHEVRON_RIGHT
                })
                .size(icon)
                .text_color(cx.theme().muted_foreground),
        ),
    )
}

/// A callout's container: a tinted box with a bar in the alert's colour.
pub fn callout_container(name: &str, cx: &mut App) -> Div {
    let theme = cx.theme();
    let accent: Hsla = match name {
        "tip" => theme.success,
        "important" => theme.primary,
        "warning" => theme.warning,
        "caution" => theme.danger,
        _ => theme.info,
    };
    div()
        .w_full()
        .rounded(px(6.))
        .border_l(px(3.))
        .border_color(accent)
        .bg(accent.opacity(0.08))
        .px(rems(0.875))
        .py(rems(0.5))
}

/// `extensions` with toggles' and GitHub's alerts' containers registered.
pub fn with_default_surfaces(extensions: MarkdownExtensions) -> MarkdownExtensions {
    let mut extensions = extensions
        .block_surface(TOGGLE_OPEN_SURFACE, |window, cx| {
            toggle_container(true, window, cx)
        })
        .block_surface(TOGGLE_CLOSED_SURFACE, |window, cx| {
            toggle_container(false, window, cx)
        });
    for name in ALERT_NAMES {
        extensions = extensions.block_surface(name, move |_, cx| callout_container(name, cx));
    }
    extensions
}
