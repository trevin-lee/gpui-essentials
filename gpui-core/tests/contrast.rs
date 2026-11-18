//! The theme's colour pairs against WCAG 2.2 AA, as `docs/contrast.md` audits them: each
//! foreground composited over the fills it's drawn on, then WCAG's contrast ratio. Text needs
//! 4.5:1 (1.4.3; all of gpui-core's text is under the large-text size). The focus ring, the
//! boundary that shows a control and chart series need 3:1 (1.4.11, 2.4.13). Disabled controls
//! (exempt) and decorative lines (`border`, `overlay_line`) aren't checked.
//!
//! A pair that drops below its level fails here with its ratio, so a colour change can't
//! quietly undo the audit.

use gpui::{Hsla, Rgba};
use gpui_core::Colors;

/// A colour from the theme.
type Pick = fn(&Colors) -> Hsla;

/// One pair: what it is, the foreground, the layers it's drawn on (bottom first, the first
/// opaque), and the ratio it needs.
struct Pair {
    what: &'static str,
    fg: Pick,
    on: Vec<Pick>,
    needs: f32,
}

const TEXT: f32 = 4.5;
const NON_TEXT: f32 = 3.0;

fn pair(what: &'static str, fg: Pick, on: Vec<Pick>, needs: f32) -> Pair {
    Pair {
        what,
        fg,
        on,
        needs,
    }
}

/// `top` over the opaque `bottom`, as WCAG takes a translucent colour: blended in sRGB.
fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let a = top.a;
    let mix = |t: f32, b: f32| t * a + b * (1. - a);
    Rgba {
        r: mix(top.r, bottom.r),
        g: mix(top.g, bottom.g),
        b: mix(top.b, bottom.b),
        a: 1.,
    }
}

fn luminance(c: Rgba) -> f32 {
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn ratio(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn hex(c: Rgba) -> String {
    let b = |x: f32| (x * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

/// The pairs both themes draw. Where a component draws differently in the dark and the light,
/// `dark` picks the dark one.
fn pairs(dark: bool) -> Vec<Pair> {
    let mut p = vec![
        // Text.
        pair(
            "foreground on background",
            |c| c.foreground,
            vec![|c| c.background],
            TEXT,
        ),
        pair(
            "card_foreground on card",
            |c| c.card_foreground,
            vec![|c| c.card],
            TEXT,
        ),
        pair(
            "popover_foreground on popover",
            |c| c.popover_foreground,
            vec![|c| c.popover],
            TEXT,
        ),
        pair(
            "secondary_foreground on secondary",
            |c| c.secondary_foreground,
            vec![|c| c.secondary],
            TEXT,
        ),
        pair(
            "primary_foreground on primary",
            |c| c.primary_foreground,
            vec![|c| c.primary],
            TEXT,
        ),
        pair(
            "accent_foreground on accent over background",
            |c| c.accent_foreground,
            vec![|c| c.background, |c| c.accent],
            TEXT,
        ),
        pair(
            "accent_foreground on accent over popover",
            |c| c.accent_foreground,
            vec![|c| c.popover, |c| c.accent],
            TEXT,
        ),
        pair(
            "tooltip: background on foreground",
            |c| c.background,
            vec![|c| c.foreground],
            TEXT,
        ),
        pair(
            "sidebar_foreground on sidebar",
            |c| c.sidebar_foreground,
            vec![|c| c.sidebar],
            TEXT,
        ),
        pair(
            "sidebar_accent_foreground on sidebar_accent over sidebar",
            |c| c.sidebar_accent_foreground,
            vec![|c| c.sidebar, |c| c.sidebar_accent],
            TEXT,
        ),
        pair(
            "sidebar group label: sidebar_foreground at 70% on sidebar",
            |c| c.sidebar_foreground.opacity(0.7),
            vec![|c| c.sidebar],
            TEXT,
        ),
        // Secondary text.
        pair(
            "muted_foreground on background",
            |c| c.muted_foreground,
            vec![|c| c.background],
            TEXT,
        ),
        pair(
            "muted_foreground on card",
            |c| c.muted_foreground,
            vec![|c| c.card],
            TEXT,
        ),
        pair(
            "muted_foreground on popover",
            |c| c.muted_foreground,
            vec![|c| c.popover],
            TEXT,
        ),
        pair(
            "muted_foreground on muted",
            |c| c.muted_foreground,
            vec![|c| c.muted],
            TEXT,
        ),
        pair(
            "placeholder on a field's fill on the page",
            |c| c.muted_foreground,
            vec![|c| c.background, |c| c.input_fill],
            TEXT,
        ),
        pair(
            "placeholder on a field's fill on a card",
            |c| c.muted_foreground,
            vec![|c| c.field_on_card],
            TEXT,
        ),
        pair(
            "muted_70 (a row's time) on sidebar",
            |c| c.muted_70,
            vec![|c| c.sidebar],
            TEXT,
        ),
        // Status text.
        pair(
            "destructive on background",
            |c| c.destructive,
            vec![|c| c.background],
            TEXT,
        ),
        pair(
            "destructive on card",
            |c| c.destructive,
            vec![|c| c.card],
            TEXT,
        ),
        pair(
            "destructive on popover",
            |c| c.destructive,
            vec![|c| c.popover],
            TEXT,
        ),
        pair(
            "alert description: destructive at 90% on card",
            |c| c.destructive.opacity(0.9),
            vec![|c| c.card],
            TEXT,
        ),
        pair(
            "success_text on background",
            |c| c.success_text,
            vec![|c| c.background],
            TEXT,
        ),
        pair(
            "success_text on card",
            |c| c.success_text,
            vec![|c| c.card],
            TEXT,
        ),
        pair(
            "warning_text on warning_fill over background",
            |c| c.warning_text,
            vec![|c| c.background, |c| c.warning_fill],
            TEXT,
        ),
        pair(
            "attention_foreground on background",
            |c| c.attention_foreground,
            vec![|c| c.background],
            TEXT,
        ),
        pair(
            "live as text on background",
            |c| c.live,
            vec![|c| c.background],
            TEXT,
        ),
        // Callouts: the colour on its own tint over the page.
        pair(
            "callout_note on its background",
            |c| c.callout_note,
            vec![|c| c.background, |c| c.callout_note_background],
            TEXT,
        ),
        pair(
            "callout_tip on its background",
            |c| c.callout_tip,
            vec![|c| c.background, |c| c.callout_tip_background],
            TEXT,
        ),
        pair(
            "callout_important on its background",
            |c| c.callout_important,
            vec![|c| c.background, |c| c.callout_important_background],
            TEXT,
        ),
        pair(
            "callout_warning on its background",
            |c| c.callout_warning,
            vec![|c| c.background, |c| c.callout_warning_background],
            TEXT,
        ),
        pair(
            "callout_caution on its background",
            |c| c.callout_caution,
            vec![|c| c.background, |c| c.callout_caution_background],
            TEXT,
        ),
        // Status marks.
        pair(
            "live dot against background",
            |c| c.live,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "attention dot against background",
            |c| c.attention,
            vec![|c| c.background],
            NON_TEXT,
        ),
        // Focus.
        pair(
            "focus ring against background",
            |c| c.focus_ring(),
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "focus ring against card",
            |c| c.focus_ring(),
            vec![|c| c.card],
            NON_TEXT,
        ),
        pair(
            "focus ring against muted (tabs, segments)",
            |c| c.focus_ring(),
            vec![|c| c.muted],
            NON_TEXT,
        ),
        pair(
            "focused field border (ring) against background",
            |c| c.ring,
            vec![|c| c.background],
            NON_TEXT,
        ),
        // A control's boundary: a field's border, a checkbox's and a radio's outline, the outline
        // button's border and the off switch's track, all `input`.
        pair(
            "input against background",
            |c| c.input,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "input against card",
            |c| c.input,
            vec![|c| c.card],
            NON_TEXT,
        ),
        // Charts.
        pair(
            "chart_1 against background",
            |c| c.chart_1,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "chart_2 against background",
            |c| c.chart_2,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "chart_3 against background",
            |c| c.chart_3,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "chart_4 against background",
            |c| c.chart_4,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "chart_5 against background",
            |c| c.chart_5,
            vec![|c| c.background],
            NON_TEXT,
        ),
        pair(
            "chart_1 against card",
            |c| c.chart_1,
            vec![|c| c.card],
            NON_TEXT,
        ),
        pair(
            "chart_2 against card",
            |c| c.chart_2,
            vec![|c| c.card],
            NON_TEXT,
        ),
        pair(
            "chart_3 against card",
            |c| c.chart_3,
            vec![|c| c.card],
            NON_TEXT,
        ),
        pair(
            "chart_4 against card",
            |c| c.chart_4,
            vec![|c| c.card],
            NON_TEXT,
        ),
        pair(
            "chart_5 against card",
            |c| c.chart_5,
            vec![|c| c.card],
            NON_TEXT,
        ),
    ];
    // Code: each syntax colour on every fill a code block sits on (design/tokens.json promises
    // 4.5:1 against background, card and secondary).
    let syntax: [(&'static str, Pick); 10] = [
        ("syntax_keyword", |c| c.syntax_keyword),
        ("syntax_string", |c| c.syntax_string),
        ("syntax_comment", |c| c.syntax_comment),
        ("syntax_function", |c| c.syntax_function),
        ("syntax_type", |c| c.syntax_type),
        ("syntax_constant", |c| c.syntax_constant),
        ("syntax_property", |c| c.syntax_property),
        ("syntax_tag", |c| c.syntax_tag),
        ("syntax_attribute", |c| c.syntax_attribute),
        ("syntax_punctuation", |c| c.syntax_punctuation),
    ];
    for (what, fg) in syntax {
        p.push(pair(what, fg, vec![|c| c.background], TEXT));
        p.push(pair(what, fg, vec![|c| c.card], TEXT));
        p.push(pair(what, fg, vec![|c| c.secondary], TEXT));
    }
    // What button.rs draws differently in each theme.
    if dark {
        p.push(pair(
            "outline button text on its fill (control at 30%)",
            |c| c.foreground,
            vec![|c| c.background, |c| c.control.opacity(0.3)],
            TEXT,
        ));
        p.push(pair(
            "selected outline or ghost button (foreground on control)",
            |c| c.foreground,
            vec![|c| c.control],
            TEXT,
        ));
        p.push(pair(
            "destructive button text on its 20% fill",
            |c| c.destructive,
            vec![|c| c.background, |c| c.destructive.opacity(0.2)],
            TEXT,
        ));
    } else {
        p.push(pair(
            "selected outline or ghost button (foreground on muted)",
            |c| c.foreground,
            vec![|c| c.muted],
            TEXT,
        ));
        p.push(pair(
            "destructive button text on its 10% fill",
            |c| c.destructive,
            vec![|c| c.background, |c| c.destructive.opacity(0.1)],
            TEXT,
        ));
    }
    p
}

/// Every pair's ratio in one theme, and the ones below their level.
fn check(theme: &str, colors: &Colors, dark: bool) -> Vec<String> {
    let mut failed = Vec::new();
    for p in pairs(dark) {
        let mut under = Rgba::from((p.on[0])(colors));
        assert_eq!(
            under.a, 1.,
            "{theme}: {}: the bottom fill must be opaque",
            p.what
        );
        for layer in &p.on[1..] {
            under = over(Rgba::from(layer(colors)), under);
        }
        let fg = over(Rgba::from((p.fg)(colors)), under);
        let r = ratio(fg, under);
        if r < p.needs {
            failed.push(format!(
                "{theme}: {} is {} on {}, {r:.2}:1, under {}:1",
                p.what,
                hex(fg),
                hex(under),
                p.needs
            ));
        }
    }
    failed
}

#[test]
fn the_dark_theme_meets_wcag_aa() {
    let failed = check("dark", &Colors::dark(), true);
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn the_light_theme_meets_wcag_aa() {
    let failed = check("light", &Colors::light(), false);
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

/// The ratio itself, against WCAG's own figures: black on white is 21:1, and a colour on
/// itself 1:1.
#[test]
fn the_ratio_is_wcags() {
    let black = gpui::rgb(0x000000);
    let white = gpui::rgb(0xffffff);
    assert!((ratio(black, white) - 21.).abs() < 0.01);
    assert!((ratio(white, white) - 1.).abs() < 0.001);
    // #767676 on white is the well-known 4.54:1.
    assert!((ratio(gpui::rgb(0x767676), white) - 4.54).abs() < 0.01);
}
