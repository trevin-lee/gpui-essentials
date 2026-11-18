//! The dark theme is the apps' reference look: `design/tokens.json`, a copy of
//! open-cloud-agents' (`design/tokens.source` says which commit; `scripts/sync-tokens.sh`
//! copies a newer one). Every colour, radius and text size there must be the theme's, and every
//! token must be either mapped here or listed as not the theme's, so a new token can't be missed.

use std::collections::BTreeSet;

use gpui::{Hsla, Rgba};
use gpui_core::{Colors, Theme};
use serde_json::Value;

type Field = (&'static str, fn(&Colors) -> Hsla);

/// Each colour token and the theme's colours that are it.
fn colours() -> Vec<(&'static str, Vec<Field>)> {
    macro_rules! f {
        ($($name:ident),*) => { vec![$((stringify!($name), (|c: &Colors| c.$name) as fn(&Colors) -> Hsla)),*] };
    }
    vec![
        ("sidebar", f!(sidebar)),
        (
            "background",
            f!(background, primary_foreground, sidebar_primary_foreground),
        ),
        (
            "foreground",
            f!(
                foreground,
                card_foreground,
                popover_foreground,
                primary,
                secondary_foreground,
                accent_foreground,
                sidebar_foreground,
                sidebar_primary,
                sidebar_accent_foreground
            ),
        ),
        ("card", f!(card, popover)),
        ("secondary", f!(secondary, muted)),
        ("muted-foreground", f!(muted_foreground)),
        ("border", f!(border, sidebar_border)),
        ("input", f!(input)),
        ("control", f!(control)),
        ("ring", f!(ring, sidebar_ring)),
        ("live", f!(live, success, ready)),
        ("attention", f!(attention, attention_foreground)),
        ("destructive", f!(destructive)),
        ("warning-text", f!(warning_text)),
        ("success-text", f!(success_text)),
        ("success-fill", f!(success_fill)),
        ("warning", f!(warning)),
        ("field-on-card", f!(field_on_card)),
        ("field-on-page", f!(field_on_page)),
        ("hover", f!(accent, sidebar_accent)),
        ("overlay-line", f!(overlay_line)),
        ("input-fill", f!(input_fill)),
        ("tile-fill", f!(tile_fill)),
        ("muted-70", f!(muted_70)),
        ("destructive-fill", f!(destructive_fill)),
        ("warning-fill", f!(warning_fill)),
        ("warning-line", f!(warning_line)),
        ("scrim", f!(scrim)),
        ("scrim-strong", f!(scrim_strong)),
        ("syntax.keyword", f!(syntax_keyword)),
        ("syntax.string", f!(syntax_string)),
        ("syntax.comment", f!(syntax_comment)),
        ("syntax.function", f!(syntax_function)),
        ("syntax.type", f!(syntax_type)),
        ("syntax.constant", f!(syntax_constant)),
        ("syntax.property", f!(syntax_property)),
        ("syntax.tag", f!(syntax_tag)),
        ("syntax.attribute", f!(syntax_attribute)),
        ("syntax.punctuation", f!(syntax_punctuation)),
        ("callout.note", f!(callout_note)),
        ("callout.note-background", f!(callout_note_background)),
        ("callout.tip", f!(callout_tip)),
        ("callout.tip-background", f!(callout_tip_background)),
        ("callout.important", f!(callout_important)),
        (
            "callout.important-background",
            f!(callout_important_background),
        ),
        ("callout.warning", f!(callout_warning)),
        ("callout.warning-background", f!(callout_warning_background)),
        ("callout.caution", f!(callout_caution)),
        ("callout.caution-background", f!(callout_caution_background)),
    ]
}

/// Tokens that aren't the theme's, and why.
const NOT_THEME: &[(&str, &str)] = &[
    (
        "font.family",
        "the apps bundle Inter and Geist Mono and set them; the theme's default stays the \
         system's fonts, which every Mac and iPhone has (a family that isn't loaded draws nothing)",
    ),
    (
        "font.size.mono",
        "code's size is TextView's and the input's",
    ),
    ("font.leading", "each component's line height"),
    ("font.weight", "each component's weight"),
    ("space", "each component's spacing"),
    ("size", "each component's size"),
    ("motion", "each component's timing"),
];

fn hex(v: &str) -> Rgba {
    let h = v.trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).unwrap_or_else(|_| panic!("not a colour: {v}"));
    match h.len() {
        6 => gpui::rgb(n),
        8 => gpui::rgba(n),
        _ => panic!("not a colour: {v}"),
    }
}

fn hex_of(c: Rgba) -> String {
    let b = |x: f32| (x * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b), b(c.a))
}

fn same(a: Rgba, b: Rgba) -> bool {
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| ((x - y) * 255.).abs() <= 1.)
}

fn px(v: &Value) -> f32 {
    v.as_str()
        .and_then(|s| s.strip_suffix("px"))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("not a dimension in px: {v}"))
}

/// Every token's path (`color.ring`, `font.size.sm`) and value.
fn tokens(v: &Value, path: &str, out: &mut Vec<(String, Value)>) {
    let Some(map) = v.as_object() else { return };
    if let Some(value) = map.get("$value") {
        out.push((path.to_string(), value.clone()));
        return;
    }
    for (k, child) in map {
        if !k.starts_with('$') {
            let p = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            tokens(child, &p, out);
        }
    }
}

#[test]
fn the_dark_theme_is_the_apps_reference_look() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(dir.join("design/tokens.json")).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    let mut all = Vec::new();
    tokens(&json, "", &mut all);

    let theme = Theme::dark();
    // Tokens are CSS pixels: the Mac app's points at its 15 point base.
    let base = 15.;
    let mut problems = Vec::new();
    let mut checked = BTreeSet::new();

    for (name, fields) in colours() {
        let path = format!("color.{name}");
        let Some((_, v)) = all.iter().find(|(p, _)| *p == path) else {
            problems.push(format!(
                "{path} isn't in tokens.json any more: drop it from the test (and its fields \
                 from Colors if nothing else uses them)"
            ));
            continue;
        };
        let want = hex(v.as_str().unwrap());
        for (field, get) in fields {
            let got: Rgba = get(&theme.colors).into();
            if !same(got, want) {
                problems.push(format!(
                    "Colors::dark().{field} is {}, but {path} is {}: make it the token's",
                    hex_of(got),
                    hex_of(want)
                ));
            }
        }
        checked.insert(path);
    }

    let radii: [(&str, f32); 5] = [
        ("sm", theme.radius_sm().0),
        ("md", theme.radius_md().0),
        ("lg", theme.radius_lg().0),
        ("xl", theme.radius_xl().0),
        ("card", theme.radius_card().0),
    ];
    let sizes: [(&str, f32); 4] = [
        ("title", theme.text.title.0),
        ("base", theme.text.base.0),
        ("sm", theme.text.sm.0),
        ("xs", theme.text.xs.0),
    ];
    for (group, list) in [("radius", &radii[..]), ("font.size", &sizes[..])] {
        for (name, rems) in list {
            let path = format!("{group}.{name}");
            match all.iter().find(|(p, _)| *p == path) {
                Some((_, v)) if (px(v) - rems * base).abs() > 0.01 => problems.push(format!(
                    "the theme's {path} is {}px, but the token is {}px",
                    rems * base,
                    px(v)
                )),
                Some(_) => {}
                None => problems.push(format!("{path} isn't in tokens.json any more")),
            }
            checked.insert(path);
        }
    }

    for (path, _) in &all {
        let listed = NOT_THEME
            .iter()
            .any(|(p, _)| path == p || path.starts_with(&format!("{p}.")));
        if !listed && !checked.contains(path) {
            problems.push(format!(
                "{path} is a token the theme doesn't have: add it to the theme and check it here, \
                 or list it in NOT_THEME with why"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "the dark theme differs from design/tokens.json ({}):\n  {}",
        std::fs::read_to_string(dir.join("design/tokens.source"))
            .unwrap_or_default()
            .trim(),
        problems.join("\n  ")
    );
}
