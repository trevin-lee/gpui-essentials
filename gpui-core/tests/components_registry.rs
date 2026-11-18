//! components.toml, which `gpui-core add` trusts, is generated from the source and checked
//! here: every module is base or a component, each component's `needs` are exactly the
//! components it uses, and it reaches nothing else in the crate but `crate::base`.
//!
//! After changing a component's paths, keys or actions, regenerate it with
//! `GPUI_CORE_WRITE_REGISTRY=1 cargo test -p gpui-core --test components_registry`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Default, Debug)]
struct Entry {
    about: String,
    file: String,
    exports: Vec<String>,
    needs: Vec<String>,
    crates: Vec<String>,
    init: Option<String>,
    contexts: Vec<String>,
    actions: Option<String>,
}

fn strings(value: &str) -> Vec<String> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The registry's subset of TOML: `key = "string"`, `key = ["strings"]` and `[tables]`.
fn registry(text: &str) -> (Vec<String>, BTreeMap<String, Entry>) {
    let mut base = Vec::new();
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut table: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            assert!(
                entries.insert(name.to_string(), Entry::default()).is_none(),
                "components.toml lists [{name}] twice"
            );
            table = Some(name.to_string());
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("components.toml line {}: expected key = value", n + 1));
        let (key, value) = (key.trim(), value.trim());
        let Some(name) = &table else {
            assert_eq!(
                key, "base",
                "components.toml: only `base` comes before the tables"
            );
            base = strings(value);
            continue;
        };
        let e = entries.get_mut(name).unwrap();
        let one = || Some(value.trim_matches('"').to_string());
        match key {
            "about" => {
                e.about = value
                    .trim_matches('"')
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
            }
            "file" => e.file = value.trim_matches('"').to_string(),
            "exports" => e.exports = strings(value),
            "needs" => e.needs = strings(value),
            "crates" => e.crates = strings(value),
            "init" => e.init = one(),
            "contexts" => e.contexts = strings(value),
            "actions" => e.actions = one(),
            other => panic!("components.toml [{name}]: unknown key `{other}`"),
        }
    }
    (base, entries)
}

/// The first segment of every `crate::` path in `text` (groups `use crate::{a, b::c}` give each
/// member's), with its line.
fn crate_paths(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("crate::") {
        // `$crate::` and the like aren't this crate's paths.
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            continue;
        }
        let line = text[..at].matches('\n').count() + 1;
        let rest = &text[at + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            let mut depth = 1;
            let mut end = 0;
            for (i, c) in group.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            let mut depth = 0;
            let mut item = String::new();
            for c in group[..end].chars().chain([',']) {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if c == ',' && depth == 0 {
                    let first: String = item
                        .trim()
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !first.is_empty() {
                        out.push((line, first));
                    }
                    item.clear();
                } else {
                    item.push(c);
                }
            }
        } else {
            let first: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            out.push((line, first));
        }
    }
    out
}

/// What lib.rs re-exports from `module` (`pub use module::{A, B};` or `pub use module::A;`).
fn exports(lib: &str, module: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in lib.match_indices(&format!("\npub use {module}::")) {
        let rest = &lib[at + format!("\npub use {module}::").len()..];
        let item = &rest[..rest.find(';').unwrap()];
        let item = item.trim().trim_start_matches('{').trim_end_matches('}');
        out.extend(
            item.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    out.sort();
    out
}

/// The members of `use crate::base::{…}` groups.
fn crate_paths_in_groups(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("use crate::base::{") {
        let rest = &text[at + "use crate::base::{".len()..];
        let group = &rest[..rest.find('}').unwrap_or(rest.len())];
        out.extend(group.split(',').map(|s| s.trim().to_string()));
    }
    out
}

/// The names of gpui-core's `[dependencies]` but GPUI's: a component using one makes the host
/// add it.
fn dependencies(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[dependencies]";
            continue;
        }
        if inside && let Some((name, _)) = line.split_once('=') {
            let name = name.trim();
            if !name.starts_with('#') && name != "gpui" && !name.starts_with("gpui-") {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// The crate's modules, from lib.rs's `mod` lines.
fn modules(lib: &str) -> BTreeSet<String> {
    lib.lines()
        .filter_map(|l| {
            let l = l.trim();
            let l = l.strip_prefix("pub ").unwrap_or(l);
            l.strip_prefix("mod ")?
                .strip_suffix(';')
                .map(str::to_string)
        })
        .collect()
}

const HEADER: &str = "\
# Generated from the source by tests/components_registry.rs, which CI runs: don't edit it by
# hand (but for `base`). Regenerate it with
# `GPUI_CORE_WRITE_REGISTRY=1 cargo test -p gpui-core --test components_registry`.
#
# The components `gpui-core add` copies into a host's crate. A component reaches the rest of
# gpui-core only through `crate::base::…` (in a copy, `gpui_core::base::…`) and through the
# components in its `needs` (in a copy, `super::…`). For each:
#   about     the first line of its module's docs
#   file      its source, from this crate's directory
#   exports   what gpui-core's root re-exports from it, which the copies' mod.rs re-exports
#   needs     the components it uses directly, which are copied with it
#   crates    gpui-core's dependencies besides gpui it uses, which the host adds; absent if none
#   init      its function binding keys, which the copies' `init` calls; absent if none
#   contexts  its own key contexts, which the CLI renames (with a `Ui` prefix)
#   actions   its action namespace, which the CLI renames (to `ui`); absent if none

# The modules that stay in gpui-core, behind gpui_core::base. Edit this by hand.
";

fn quoted(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|i| format!("\"{i}\"")).collect();
    format!("[{}]", items.join(", "))
}

/// The registry's text for `base` and the components' entries.
fn render(base: &BTreeSet<String>, entries: &BTreeMap<String, Entry>) -> String {
    let base: Vec<String> = base.iter().cloned().collect();
    let mut out = format!("{HEADER}base = {}\n", quoted(&base));
    for (name, e) in entries {
        out += &format!("\n[{name}]\n");
        out += &format!(
            "about = \"{}\"\n",
            e.about.replace('\\', "\\\\").replace('"', "\\\"")
        );
        out += &format!("file = \"{}\"\n", e.file);
        out += &format!("exports = {}\n", quoted(&e.exports));
        out += &format!("needs = {}\n", quoted(&e.needs));
        if !e.crates.is_empty() {
            out += &format!("crates = {}\n", quoted(&e.crates));
        }
        if let Some(init) = &e.init {
            out += &format!("init = \"{init}\"\n");
        }
        out += &format!("contexts = {}\n", quoted(&e.contexts));
        if let Some(actions) = &e.actions {
            out += &format!("actions = \"{actions}\"\n");
        }
    }
    out
}

/// The first sentence of a module's `//!` docs, without markdown links.
fn about(text: &str) -> String {
    let mut first = String::new();
    for line in text.lines() {
        let Some(l) = line.strip_prefix("//!") else {
            break;
        };
        let l = l.trim();
        if l.is_empty() {
            break;
        }
        if !first.is_empty() {
            first.push(' ');
        }
        first.push_str(l);
    }
    let first = first
        .split(". ")
        .next()
        .unwrap_or(&first)
        .trim_end_matches('.');
    // `[`X`](path)` and `[`X`]` read as `X`.
    let mut out = String::new();
    let mut rest = first;
    while let Some(at) = rest.find('[') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let Some(end) = after.find(']') else {
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        out.push_str(&after[..end]);
        rest = &after[end + 1..];
        if let Some(link) = rest.strip_prefix('(') {
            rest = link.find(')').map_or(link, |e| &link[e + 1..]);
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn components_toml_matches_the_source() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |p: &str| {
        std::fs::read_to_string(dir.join(p)).unwrap_or_else(|e| panic!("reading {p}: {e}"))
    };
    let checked_in = read("components.toml");
    let (base, _) = registry(&checked_in);
    let base: BTreeSet<String> = base.into_iter().collect();
    let mut problems = Vec::new();
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();

    for b in &base {
        if !dir.join(format!("src/{b}.rs")).exists()
            && !dir.join(format!("src/{b}/mod.rs")).exists()
        {
            problems.push(format!(
                "`base` lists `{b}`, which has no src/{b}.rs: remove it"
            ));
        }
    }
    let lib = read("src/lib.rs");
    let deps = dependencies(&read("Cargo.toml"));
    let components: Vec<String> = modules(&lib)
        .into_iter()
        .filter(|m| !base.contains(m))
        .collect();

    for name in &components {
        let path = format!("src/{name}.rs");
        let Ok(text) = std::fs::read_to_string(dir.join(&path)) else {
            problems.push(format!(
                "src/{name}/ is a directory: a component is one file (or put `{name}` in `base`)"
            ));
            continue;
        };

        let mut used = BTreeSet::new();
        for (line, first) in crate_paths(&text) {
            if first == "base" || first == *name {
                continue;
            }
            if components.contains(&first) {
                used.insert(first);
            } else {
                problems.push(format!(
                    "{path}:{line} uses crate::{first}: a component reaches gpui-core only \
                     through crate::base (add what it needs to src/base.rs) and other \
                     components through their own modules"
                ));
            }
        }
        if text.contains("use crate::base::init")
            || crate_paths_in_groups(&text)
                .iter()
                .any(|i| i == "init" || i == "init_soon")
        {
            problems.push(format!(
                "{path} imports base's `init` or `init_soon`: call them by their whole path \
                 (`crate::base::init_soon(cx)`), which a copy rewrites to its own"
            ));
        }
        for (n, line) in text.lines().enumerate() {
            if line.starts_with("use super::") {
                problems.push(format!(
                    "{path}:{}: `super::` at the top of a component is the crate root: use \
                     crate::base",
                    n + 1
                ));
            }
        }

        let binds = text.contains("pub(crate) fn init(") && text.contains("bind_keys");
        let contexts: Vec<String> = text
            .match_indices(": &str = \"GpuiCore")
            .map(|(at, _)| {
                let s = &text[at + ": &str = \"".len()..];
                s[..s.find('"').unwrap()].to_string()
            })
            .collect();
        let actions = text.find("actions!(").map(|at| {
            text[at + "actions!(".len()..]
                .trim_start()
                .split(|c: char| c == ',' || c.is_whitespace())
                .next()
                .unwrap()
                .to_string()
        });
        if actions.is_some() && !binds {
            problems.push(format!(
                "{path} defines actions but no `pub(crate) fn init` binding keys: the copies' \
                 init wouldn't bind them"
            ));
        }
        let about = about(&text);
        if about.is_empty() {
            problems.push(format!("{path} has no `//!` docs to describe it"));
        }
        entries.insert(
            name.clone(),
            Entry {
                about,
                file: path,
                exports: exports(&lib, name),
                needs: used.into_iter().collect(),
                crates: deps
                    .iter()
                    .filter(|d| text.contains(&format!("{}::", d.replace('-', "_"))))
                    .cloned()
                    .collect(),
                init: binds.then(|| "init".to_string()),
                contexts,
                actions,
            },
        );
    }

    assert!(
        problems.is_empty(),
        "a component reaches what a copy can't (the `gpui-core add` CLI copies each file \
         alone):\n  {}",
        problems.join("\n  ")
    );

    let generated = render(&base, &entries);
    if std::env::var_os("GPUI_CORE_WRITE_REGISTRY").is_some() {
        std::fs::write(dir.join("components.toml"), &generated).unwrap();
        return;
    }
    // The parser reads back what the generator writes.
    let (_, parsed) = registry(&generated);
    assert_eq!(parsed.len(), entries.len());
    for (name, e) in &entries {
        let p = &parsed[name];
        assert_eq!(
            (
                &p.file,
                &p.exports,
                &p.needs,
                &p.crates,
                &p.init,
                &p.contexts,
                &p.actions,
                &p.about
            ),
            (
                &e.file,
                &e.exports,
                &e.needs,
                &e.crates,
                &e.init,
                &e.contexts,
                &e.actions,
                &e.about
            ),
            "[{name}] round trip"
        );
    }
    if generated != checked_in {
        let first = generated
            .lines()
            .zip(checked_in.lines())
            .position(|(a, b)| a != b)
            .map_or(String::new(), |n| {
                format!(
                    "\nfirst difference, line {}:\n  source says: {}\n  file has:    {}",
                    n + 1,
                    generated.lines().nth(n).unwrap_or(""),
                    checked_in.lines().nth(n).unwrap_or("")
                )
            });
        panic!(
            "components.toml is out of date with gpui-core/src (the `gpui-core add` CLI trusts \
             it): regenerate it with `GPUI_CORE_WRITE_REGISTRY=1 cargo test -p gpui-core --test \
             components_registry`{first}"
        );
    }
}
