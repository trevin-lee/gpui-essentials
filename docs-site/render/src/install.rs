//! Each component page's Installation section, from gpui-core's components.toml: what
//! `gpui-core add` copies into a host's crate. A component listed there is copied by its name,
//! with the components it needs and the crates it uses; one that stays in gpui-core (its
//! `base`: the theme, popovers, inputs, charts…) is used from the dependency alone.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result};

const REPO: &str = "https://github.com/trevin-lee/gpui-essentials";

/// What `gpui-core add` does for one component.
pub struct Copy {
    /// Its name to the CLI, hyphenated: `date-picker`.
    name: String,
    /// The components copied with it, by their CLI names.
    needs: Vec<String>,
    /// gpui-core's dependencies it uses, which the host adds.
    crates: Vec<String>,
}

/// Every copyable component, under each name gpui-core's root exports from it (`Button`,
/// `ButtonGroup`…).
pub struct Installs(HashMap<String, std::rc::Rc<Copy>>);

fn cli_name(module: &str) -> String {
    module.replace('_', "-")
}

fn strings(entry: &toml::Table, key: &str) -> Vec<String> {
    entry
        .get(key)
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

impl Installs {
    /// Reads `components.toml`.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let table: toml::Table =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let mut by_export = HashMap::new();
        for (module, entry) in &table {
            let Some(entry) = entry.as_table() else {
                continue; // `base`, a list
            };
            let copy = std::rc::Rc::new(Copy {
                name: cli_name(module),
                needs: strings(entry, "needs")
                    .iter()
                    .map(|m| cli_name(m))
                    .collect(),
                crates: strings(entry, "crates"),
            });
            for export in strings(entry, "exports") {
                by_export.insert(export, copy.clone());
            }
        }
        Ok(Self(by_export))
    }

    /// The Installation section for the component whose rustdoc page is `docs`
    /// (`gpui_core/struct.DatePicker.html`).
    pub fn section(&self, docs: &str) -> String {
        let export = docs
            .rsplit('/')
            .next()
            .and_then(|page| page.split('.').nth(1))
            .unwrap_or_default();
        let dependency = format!("[dependencies]\ngpui-core = {{ git = \"{REPO}\" }}");
        let mut html = String::from(r#"<section id="installation"><h2>Installation</h2>"#);
        match self.0.get(export) {
            Some(copy) => {
                let _ = write!(
                    html,
                    "<p>Copy it into your crate, as code you own, with the gpui-core CLI:</p>\
                     <pre><code>cargo install --locked --git {REPO} gpui-core-cli\n\
                     gpui-core add {}</code></pre>",
                    copy.name
                );
                if !copy.needs.is_empty() {
                    let _ = write!(
                        html,
                        r#"<p class="note">It also copies {}, which it uses.</p>"#,
                        copy.needs.join(", ")
                    );
                }
                if !copy.crates.is_empty() {
                    let _ = write!(
                        html,
                        r#"<p class="note">Add {} to your crate's dependencies too.</p>"#,
                        copy.crates
                            .iter()
                            .map(|c| format!("<code>{c}</code>"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let _ = write!(
                    html,
                    "<p>Or use it from gpui-core, as a dependency:</p><pre><code>{dependency}</code></pre>"
                );
            }
            None => {
                let _ = write!(
                    html,
                    "<p>It's part of gpui-core itself: depend on gpui-core.</p><pre><code>{dependency}</code></pre>"
                );
            }
        }
        html.push_str("</section>\n");
        html
    }
}
