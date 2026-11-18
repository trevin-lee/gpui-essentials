//! The site's pages: plain HTML with relative links, one stylesheet and a few lines of script
//! for the theme toggle.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::install::Installs;
use crate::shot::STAGE_WIDTH;
use crate::site::{Component, Crate, Family};

const STYLE: &str = include_str!("../../static/style.css");
const REPO: &str = "https://github.com/trevin-lee/gpui-essentials";

/// Turns the toggle's choice into a class on `<html>`, and points every picture's dark source
/// at it. With no choice saved, the system's setting decides.
const THEME_SCRIPT: &str = r#"<script>
(function () {
  var key = "gpui-essentials-theme";
  var root = document.documentElement;
  var system = matchMedia("(prefers-color-scheme: dark)");
  function apply(theme) {
    root.classList.remove("light", "dark");
    if (theme) root.classList.add(theme);
    var media = theme === "dark" ? "all" : theme === "light" ? "not all" : "(prefers-color-scheme: dark)";
    document.querySelectorAll("source[data-dark]").forEach(function (s) { s.media = media; });
  }
  var saved = null;
  try { saved = localStorage.getItem(key); } catch (e) {}
  apply(saved);
  document.getElementById("theme").addEventListener("click", function () {
    var dark = root.classList.contains("dark") || (!root.classList.contains("light") && system.matches);
    var next = dark ? "light" : "dark";
    apply(next);
    try { localStorage.setItem(key, next); } catch (e) {}
  });
})();
</script>"#;

/// Sets the saved theme's class before the page draws, so it doesn't flash the other one.
const EARLY_THEME_SCRIPT: &str = r#"<script>try { var t = localStorage.getItem("gpui-essentials-theme"); if (t) document.documentElement.classList.add(t); } catch (e) {}</script>"#;

/// Writes `index.html`, a page per component and per crate, and `style.css` into `out`.
pub fn write_site(
    out: &Path,
    components: &[Component],
    crates: &[Crate],
    installs: &Installs,
) -> Result<()> {
    fs::create_dir_all(out.join("components"))?;
    fs::create_dir_all(out.join("crates"))?;
    fs::write(out.join("style.css"), STYLE)?;
    let nav = |root: &str, current: Option<&str>| sidebar(root, components, crates, current);
    let index = page(
        "Components",
        "",
        &nav("", None),
        &index_body(out, components, crates),
    );
    fs::write(out.join("index.html"), index)?;
    for c in components {
        let current = format!("components/{}", c.slug);
        let html = page(
            c.name,
            "../",
            &nav("../", Some(&current)),
            &component_body(out, c, installs),
        );
        let path = out.join("components").join(format!("{}.html", c.slug));
        fs::write(&path, html).with_context(|| format!("writing {}", path.display()))?;
    }
    for c in crates {
        let current = format!("crates/{}", c.slug);
        let html = page(
            c.name,
            "../",
            &nav("../", Some(&current)),
            &crate_body(out, c),
        );
        let path = out.join("crates").join(format!("{}.html", c.slug));
        fs::write(&path, html).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Escapes text for HTML.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The families that have components, each with its components by name.
fn by_family(components: &[Component]) -> Vec<(Family, Vec<&Component>)> {
    Family::ALL
        .iter()
        .filter_map(|&family| {
            let mut list: Vec<&Component> =
                components.iter().filter(|c| c.family == family).collect();
            list.sort_by_key(|c| c.name);
            (!list.is_empty()).then_some((family, list))
        })
        .collect()
}

/// The sidebar's links: the components by family, then the crates. `root` leads from the page
/// to the site's root ("" or "../"); `current` is the page's own path without `.html`
/// (`components/button`), marked.
fn sidebar(
    root: &str,
    components: &[Component],
    crates: &[Crate],
    current: Option<&str>,
) -> String {
    let mut nav = String::new();
    let link = |nav: &mut String, path: String, name: &str| {
        let here = if current == Some(path.as_str()) {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            nav,
            r#"<li><a href="{root}{path}.html"{here}>{}</a></li>"#,
            esc(name)
        );
    };
    for (family, list) in by_family(components) {
        let _ = write!(nav, "<h4>{}</h4><ul>", family.name());
        for c in list {
            link(&mut nav, format!("components/{}", c.slug), c.name);
        }
        nav.push_str("</ul>");
    }
    nav.push_str("<h4>Crates</h4><ul>");
    for c in crates {
        link(&mut nav, format!("crates/{}", c.slug), c.name);
    }
    nav.push_str("</ul>");
    nav
}

/// A page: the top bar, the sidebar's `nav` and `body`. `root` leads from the page to the
/// site's root ("" or "../").
fn page(title: &str, root: &str, nav: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · gpui-essentials</title>
<link rel="stylesheet" href="{root}style.css">
{EARLY_THEME_SCRIPT}
</head>
<body>
<header class="topbar">
  <a class="brand" href="{root}index.html">gpui-essentials</a>
  <nav class="links">
    <a href="{root}api/gpui_core/index.html">API</a>
    <a href="{REPO}">GitHub</a>
    <button id="theme" type="button" aria-label="Switch between light and dark">
      <span class="sun" aria-hidden="true">☀</span><span class="moon" aria-hidden="true">☾</span>
    </button>
  </nav>
</header>
<div class="layout">
<aside class="sidebar">
<a class="all" href="{root}index.html">All components</a>
{nav}
</aside>
<main>
{body}
</main>
</div>
{THEME_SCRIPT}
</body>
</html>
"#,
        title = esc(title),
    )
}

/// A variant's picture: the light one, or the dark one where the page is dark.
fn picture(out: &Path, root: &str, component: &Component, variant: &str, alt: &str) -> String {
    let base = format!("img/{}/{variant}", component.slug);
    // The pictures are the stage's width; the size given keeps the page from jumping as they
    // load. Without the picture (a render `--only` another component) the page leaves it out.
    let size = image::image_dimensions(out.join(format!("{base}-light.png")))
        .map(|(w, h)| {
            let width = STAGE_WIDTH as u32;
            format!(r#" width="{width}" height="{}""#, h * width / w.max(1))
        })
        .unwrap_or_default();
    format!(
        r#"<picture>
  <source srcset="{root}{base}-dark.png" media="(prefers-color-scheme: dark)" data-dark>
  <img src="{root}{base}-light.png"{size} alt="{alt}" loading="lazy">
</picture>"#,
        alt = esc(alt),
    )
}

fn index_body(out: &Path, components: &[Component], crates: &[Crate]) -> String {
    let mut body = String::from(
        r#"<h1>Components</h1>
<p class="lead">GPUI components for macOS and iOS apps, drawn in the host's theme. Each page shows
every variant in the light and the dark theme, beside the code that drew it.</p>
"#,
    );
    for (family, list) in by_family(components) {
        let _ = write!(
            body,
            r#"<section class="family"><h2>{}</h2><div class="grid">"#,
            family.name()
        );
        for c in list {
            let thumb = c
                .variants
                .first()
                .map(|v| picture(out, "", c, v.slug, &format!("{}, {}", c.name, v.name)))
                .unwrap_or_default();
            let _ = write!(
                body,
                r#"<a class="tile" href="components/{slug}.html"><div class="thumb">{thumb}</div><div class="tile-text"><strong>{name}</strong><span>{about}</span></div></a>"#,
                slug = c.slug,
                name = esc(c.name),
                about = esc(c.about),
            );
        }
        body.push_str("</div></section>\n");
    }
    body.push_str(
        r#"<section class="family"><h2>Crates</h2><p class="lead">Larger pieces built on these components, each in its own crate.</p><div class="grid">"#,
    );
    for c in crates {
        let thumb = c
            .pictures
            .first()
            .map(|(file, alt)| crate_picture(out, "", c, file, alt))
            .unwrap_or_default();
        let _ = write!(
            body,
            r#"<a class="tile" href="crates/{slug}.html"><div class="thumb">{thumb}</div><div class="tile-text"><strong>{name}</strong><span>{about}</span></div></a>"#,
            slug = c.slug,
            name = esc(c.name),
            about = esc(c.about),
        );
    }
    body.push_str("</div></section>\n");
    body
}

/// One of a crate's pictures, at its size (halved: they're drawn at 2×).
fn crate_picture(out: &Path, root: &str, c: &Crate, file: &str, alt: &str) -> String {
    let path = format!("img/crates/{}/{file}", c.slug);
    let size = image::image_dimensions(out.join(&path))
        .map(|(w, h)| format!(r#" width="{}" height="{}""#, w / 2, h / 2))
        .unwrap_or_default();
    format!(
        r#"<img src="{root}{path}"{size} alt="{}" loading="lazy">"#,
        esc(alt)
    )
}

fn crate_body(out: &Path, c: &Crate) -> String {
    let mut body = format!(
        r#"<p class="crumb">Crates</p>
<h1>{name}</h1>
<p class="lead">{about}</p>
<p class="actions"><a class="button" href="../api/{lib}/index.html">Docs</a> <a class="button" href="{REPO}/tree/main/{dir}#readme">README</a></p>
"#,
        name = esc(c.name),
        about = esc(c.about),
        lib = c.lib,
        dir = c.dir,
    );
    for (file, alt) in &c.pictures {
        let _ = writeln!(
            body,
            r#"<figure class="shot">{}<figcaption>{}</figcaption></figure>"#,
            crate_picture(out, "../", c, file, alt),
            esc(alt)
        );
    }
    body
}

fn component_body(out: &Path, c: &Component, installs: &Installs) -> String {
    let mut body = format!(
        r#"<p class="crumb">{family}</p>
<h1>{name}</h1>
<p class="lead">{about}</p>
<p class="actions"><a class="button" href="../api/{docs}">Docs</a></p>
"#,
        family = c.family.name(),
        name = esc(c.name),
        about = esc(c.about),
        docs = c.docs,
    );
    body.push_str(&installs.section(c.docs));
    body.push_str(
        r#"<p class="note">Each example is written with <code>use gpui::*;</code> and <code>use gpui_core::*;</code> in scope, and <code>window</code> and <code>cx</code> from the <code>render</code> it's in.</p>
"#,
    );
    for v in &c.variants {
        let alt = format!("{}, {}: drawn by the code below", c.name, v.name);
        let kind = if v.demo.is_view() {
            r#"<p class="note">A view with state of its own: build it with <code>cx.new(|cx| Example::new(window, cx))</code>.</p>"#
        } else {
            ""
        };
        let _ = write!(
            body,
            r##"<section class="variant" id="{slug}">
<h2><a href="#{slug}">{name}</a></h2>
{kind}
<div class="example">
<div class="preview">{picture}</div>
<pre class="code"><code>{code}</code></pre>
</div>
</section>
"##,
            slug = v.slug,
            name = esc(v.name),
            picture = picture(out, "../", c, v.slug, &alt),
            code = esc(v.code.trim_end()),
        );
    }
    body
}
