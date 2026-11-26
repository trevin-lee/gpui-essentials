//! The registry as JSON (`render --json <file>`), for the Next.js site (docs-site/web): every
//! component with its variants' code and its installation, and the crates' pages. Writing it
//! draws nothing, so it runs anywhere the renderer builds.

use std::path::Path;

use anyhow::{Context as _, Result};
use docs_site_registry::{Component, Crate, Family};
use gpui::{Hsla, Rgba};
use gpui_core::Theme;
use serde_json::{Value, json};

use crate::install::{Installs, REPO};

/// Writes the registry to `path`.
pub fn write(
    path: &Path,
    components: &[Component],
    crates: &[Crate],
    installs: &Installs,
) -> Result<()> {
    let registry = json!({
        "repo": REPO,
        // The colour behind every picture and live preview: the theme's background.
        "stage": {
            "light": hex(Theme::light().background),
            "dark": hex(Theme::dark().background),
        },
        "families": Family::ALL.iter().map(|f| f.name()).collect::<Vec<_>>(),
        "components": components.iter().map(|c| component(c, installs)).collect::<Vec<_>>(),
        "crates": crates.iter().map(krate).collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&registry)?;
    std::fs::write(path, text + "\n").with_context(|| format!("writing {}", path.display()))
}

fn component(c: &Component, installs: &Installs) -> Value {
    json!({
        "family": c.family.name(),
        "name": c.name,
        "slug": c.slug,
        "about": c.about,
        "docs": c.docs,
        // What `gpui add` copies, or null for a component that stays in gpui-core.
        "copy": installs.copy(c.docs).map(|copy| json!({
            "name": copy.name,
            "needs": copy.needs,
            "crates": copy.crates,
        })),
        "variants": c.variants.iter().map(|v| json!({
            "name": v.name,
            "slug": v.slug,
            "kind": if v.demo.is_view() { "view" } else { "demo" },
            "code": v.code,
        })).collect::<Vec<_>>(),
    })
}

fn krate(c: &Crate) -> Value {
    json!({
        "name": c.name,
        "slug": c.slug,
        "dir": c.dir,
        "lib": c.lib,
        "about": c.about,
        "pictures": c.pictures.iter().map(|(file, alt)| json!({ "file": file, "alt": alt })).collect::<Vec<_>>(),
    })
}

/// A colour as CSS's `#rrggbb`.
fn hex(color: Hsla) -> String {
    let Rgba { r, g, b, .. } = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}
