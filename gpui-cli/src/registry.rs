//! gpui-core's components.toml: which components there are, their files, what they need and
//! what the rewrite renames. It's generated from gpui-core's source (and checked by its tests),
//! in a subset of TOML this reads without a TOML crate.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug, Clone)]
pub struct Component {
    pub about: String,
    /// From gpui-core's directory, as `src/button.rs`.
    pub file: String,
    pub exports: Vec<String>,
    pub needs: Vec<String>,
    /// Crates besides gpui and gpui-core its code uses.
    pub crates: Vec<String>,
    /// Its function binding keys, if any.
    pub init: Option<String>,
    pub contexts: Vec<String>,
    pub actions: Option<String>,
}

#[derive(Debug)]
pub struct Registry {
    pub components: BTreeMap<String, Component>,
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    let v = v
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(v);
    v.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn strings(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Registry {
    pub fn parse(text: &str) -> Result<Registry, String> {
        let mut components: BTreeMap<String, Component> = BTreeMap::new();
        let mut table: Option<String> = None;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                components.insert(name.to_string(), Component::default());
                table = Some(name.to_string());
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!(
                    "components.toml line {}: expected key = value",
                    n + 1
                ));
            };
            let (key, value) = (key.trim(), value.trim());
            let Some(name) = &table else {
                // `base`: the modules that stay in gpui-core.
                continue;
            };
            let c = components.get_mut(name).unwrap();
            match key {
                "about" => c.about = unquote(value),
                "file" => c.file = unquote(value),
                "exports" => c.exports = strings(value),
                "needs" => c.needs = strings(value),
                "crates" => c.crates = strings(value),
                "init" => c.init = Some(unquote(value)),
                "contexts" => c.contexts = strings(value),
                "actions" => c.actions = Some(unquote(value)),
                // A newer registry's keys: this CLI doesn't need them.
                _ => {}
            }
        }
        if components.is_empty() {
            return Err("components.toml lists no components".into());
        }
        Ok(Registry { components })
    }

    /// `names` and everything they need, all the way down.
    pub fn closure(&self, names: &[String]) -> Result<BTreeSet<String>, String> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<String> = names.to_vec();
        while let Some(name) = stack.pop() {
            let Some(c) = self.components.get(&name) else {
                return Err(unknown(self, &name));
            };
            if out.insert(name.clone()) {
                stack.extend(c.needs.iter().cloned());
            }
        }
        Ok(out)
    }
}

fn unknown(registry: &Registry, name: &str) -> String {
    let close: Vec<&str> = registry
        .components
        .keys()
        .filter(|k| k.contains(name) || name.contains(k.as_str()))
        .map(String::as_str)
        .collect();
    let hint = if close.is_empty() {
        "`gpui list` shows them".to_string()
    } else {
        format!("did you mean {}?", close.join(", "))
    };
    format!("gpui-core has no component `{name}` ({hint})")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# comment
base = ["theme"]

[button]
about = "Buttons, \"as\" the web draws them"
file = "src/button.rs"
exports = ["Button", "ButtonVariant"]
needs = []
contexts = []

[dialog]
about = "A dialog"
file = "src/dialog.rs"
exports = ["Dialog"]
needs = ["button"]
init = "init"
contexts = ["GpuiCoreDialog"]
actions = "gpui_core_dialog"
future = "ignored"

[command]
file = "src/command.rs"
needs = ["dialog"]
contexts = []
"#;

    #[test]
    fn parses_the_subset() {
        let r = Registry::parse(SAMPLE).unwrap();
        assert_eq!(r.components.len(), 3);
        let b = &r.components["button"];
        assert_eq!(b.about, "Buttons, \"as\" the web draws them");
        assert_eq!(b.exports, ["Button", "ButtonVariant"]);
        let d = &r.components["dialog"];
        assert_eq!(d.init.as_deref(), Some("init"));
        assert_eq!(d.actions.as_deref(), Some("gpui_core_dialog"));
        assert_eq!(d.contexts, ["GpuiCoreDialog"]);
    }

    #[test]
    fn closure_takes_what_each_needs() {
        let r = Registry::parse(SAMPLE).unwrap();
        let all = r.closure(&["command".into()]).unwrap();
        assert_eq!(
            all.into_iter().collect::<Vec<_>>(),
            ["button", "command", "dialog"]
        );
        assert!(r.closure(&["dialg".into()]).is_err());
    }
}
