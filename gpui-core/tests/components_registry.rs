//! components.toml, which `gpui-core add` trusts, matches the source: every module is base or a
//! listed component, each component's `needs` are exactly the components it uses, and it reaches
//! nothing else in the crate but `crate::base`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Default, Debug)]
struct Entry {
    file: String,
    needs: Vec<String>,
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
            "file" => e.file = value.trim_matches('"').to_string(),
            "needs" => e.needs = strings(value),
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

#[test]
fn components_toml_matches_the_source() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |p: &str| {
        std::fs::read_to_string(dir.join(p)).unwrap_or_else(|e| panic!("reading {p}: {e}"))
    };
    let (base, entries) = registry(&read("components.toml"));
    let base: BTreeSet<String> = base.into_iter().collect();
    let mut problems = Vec::new();

    for m in modules(&read("src/lib.rs")) {
        match (base.contains(&m), entries.contains_key(&m)) {
            (true, true) => problems.push(format!(
                "`{m}` is both in `base` and a component: keep it in one"
            )),
            (false, false) => problems.push(format!(
                "src/{m}.rs isn't in components.toml: add a [{m}] table (or put it in `base` if \
                 it stays in gpui-core and components reach it through crate::base)"
            )),
            _ => {}
        }
    }
    for b in &base {
        if !dir.join(format!("src/{b}.rs")).exists()
            && !dir.join(format!("src/{b}/mod.rs")).exists()
        {
            problems.push(format!(
                "`base` lists `{b}`, which has no src/{b}.rs: remove it"
            ));
        }
    }

    for (name, e) in &entries {
        let path = format!("src/{name}.rs");
        if e.file != path {
            problems.push(format!(
                "[{name}] file is \"{}\": make it \"{path}\"",
                e.file
            ));
        }
        let Ok(text) = std::fs::read_to_string(dir.join(&path)) else {
            problems.push(format!(
                "[{name}]'s file {path} doesn't exist: remove the table"
            ));
            continue;
        };

        let mut used = BTreeSet::new();
        for (line, first) in crate_paths(&text) {
            if first == "base" || first == *name {
                continue;
            }
            if entries.contains_key(&first) {
                used.insert(first);
            } else {
                problems.push(format!(
                    "{path}:{line} uses crate::{first}: a component reaches gpui-core only \
                     through crate::base (add what it needs to src/base.rs) and other \
                     components through their own modules"
                ));
            }
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
        let listed: BTreeSet<String> = e.needs.iter().cloned().collect();
        for missing in used.difference(&listed) {
            problems.push(format!(
                "[{name}] uses the component `{missing}`: add it to its needs"
            ));
        }
        for extra in listed.difference(&used) {
            problems.push(format!(
                "[{name}] needs `{extra}`, which it doesn't use: remove it from its needs"
            ));
        }

        let binds = text.contains("pub(crate) fn init(") && text.contains("bind_keys");
        match (&e.init, binds) {
            (None, true) => problems.push(format!(
                "[{name}] binds keys in `init`: add init = \"init\""
            )),
            (Some(f), false) => problems.push(format!(
                "[{name}] has init = \"{f}\" but binds no keys there: remove it"
            )),
            (Some(f), true) if f != "init" => {
                problems.push(format!("[{name}]'s init is `init`, not `{f}`"))
            }
            _ => {}
        }

        let contexts: Vec<String> = text
            .match_indices(": &str = \"GpuiCore")
            .map(|(at, _)| {
                let s = &text[at + ": &str = \"".len()..];
                s[..s.find('"').unwrap()].to_string()
            })
            .collect();
        if contexts != e.contexts {
            problems.push(format!(
                "[{name}] defines the key contexts {contexts:?}: make its contexts that"
            ));
        }

        let actions = text.find("actions!(").map(|at| {
            text[at + "actions!(".len()..]
                .trim_start()
                .split(|c: char| c == ',' || c.is_whitespace())
                .next()
                .unwrap()
                .to_string()
        });
        if actions != e.actions {
            problems.push(format!(
                "[{name}] defines actions in {actions:?}: make its actions that"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "components.toml is out of date with gpui-core/src (the `gpui-core add` CLI trusts it):\n  {}",
        problems.join("\n  ")
    );
}
