//! `gpui add`: writes the copies, their mod.rs, and says what's left for the host to do.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::registry::{Component, Registry};
use crate::rewrite::{Names, rewrite};
use crate::sha256;
use crate::source::Source;

/// Each copy's first line starts so; mod.rs's says it's generated.
const MARK: &str = "// gpui add: ";
const MOD_MARK: &str = "//! Components copied from gpui-core by `gpui add`.";
/// What the CLI wrote before it was renamed `gpui` (from `gpui-core`): copies and a mod.rs made
/// then are still recognised, and get the new marks when next written.
const OLD_MARK: &str = "// gpui-core add: ";
const OLD_MOD_MARK: &str = "//! Components copied from gpui-core by `gpui-core add`.";

pub struct Request {
    pub names: Vec<String>,
    pub all: bool,
    /// Where the copies go, and how to name it to the host.
    pub dir: PathBuf,
    pub dir_shown: PathBuf,
    pub namespace: String,
    pub diff: bool,
    pub force: bool,
}

/// A copy's header: its component, where it came from, and the hash of the text below it as
/// written, so a later run can tell whether the host changed it.
fn header(name: &str, from: &str, file: &str, body: &str) -> String {
    format!(
        "{MARK}{name} from {from} ({file}), sha256 {}\n\
         // Yours to change. `gpui add {} --diff` shows what a newer gpui-core changed, and\n\
         // --force takes it, replacing your edits.\n",
        sha256::hex(body.as_bytes()),
        name.replace('_', "-"),
    )
}

/// A copy already in the directory.
struct Existing {
    text: String,
    /// The text below the header.
    body: String,
    /// The hash its header records.
    hash: String,
}

/// The component a file is a copy of, if `gpui add` wrote it, and the file.
fn existing(path: &Path) -> Option<(String, Existing)> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().next()?;
    let first = line
        .strip_prefix(MARK)
        .or_else(|| line.strip_prefix(OLD_MARK))?;
    let name = first.split(' ').next()?.to_string();
    let hash = first.rsplit("sha256 ").next()?.trim().to_string();
    let body = text.splitn(4, '\n').nth(3).unwrap_or("").to_string();
    Some((name, Existing { text, body, hash }))
}

/// The copies in `dir`, by component.
fn copies(dir: &Path) -> BTreeMap<String, Existing> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs")
            && path.file_name().is_some_and(|f| f != "mod.rs")
            && let Some((name, e)) = existing(&path)
        {
            out.insert(name, e);
        }
    }
    out
}

pub fn add(req: &Request, root: &Path, source: &Source) -> Result<(), String> {
    let registry = Registry::parse(&source.read("components.toml")?)?;
    let wanted: Vec<String> = if req.all {
        registry.components.keys().cloned().collect()
    } else {
        req.names.clone()
    };
    let closure = registry.closure(&wanted)?;
    let have = copies(&req.dir);

    let mod_path = req.dir.join("mod.rs");
    if let Ok(text) = std::fs::read_to_string(&mod_path)
        && !text.starts_with(MOD_MARK)
        && !text.starts_with(OLD_MOD_MARK)
        && !req.force
        && !req.diff
    {
        return Err(format!(
            "{} isn't the one gpui add writes, and add rewrites it: move it (or pass \
             --dir for another directory, or --force to replace it)",
            req.dir_shown.join("mod.rs").display()
        ));
    }

    // Everything that will be in the directory: its contexts and actions are renamed in every
    // copy, and its paths become `super::`.
    let all: BTreeMap<&String, &Component> = registry
        .components
        .iter()
        .filter(|(n, _)| closure.contains(*n) || have.contains_key(*n))
        .collect();
    let names = Names::new(&req.namespace, all.iter().map(|(n, c)| (*n, *c)));

    let from = source.describe();
    let mut wrote = Vec::new();
    let mut kept = Vec::new();
    let mut same = Vec::new();
    let mut diffs = 0;
    for name in &closure {
        let c = &registry.components[name];
        let body = rewrite(&c.file, &source.read(&c.file)?, &names)?;
        let text = header(name, &from, &c.file, &body) + &body;
        let path = req.dir.join(format!("{name}.rs"));
        let shown = req.dir_shown.join(format!("{name}.rs"));
        let old = std::fs::read_to_string(&path).ok();
        let copy = have.get(name);
        match (old, copy) {
            (None, _) => {
                if req.diff {
                    println!("{}: not copied yet", shown.display());
                } else {
                    write(&path, &text)?;
                    wrote.push(name.clone());
                }
            }
            (Some(_), Some(e)) if e.body == body => {
                // The same code: at most the header's revision is new.
                if e.text != text && !req.diff {
                    write(&path, &text)?;
                }
                same.push(name.clone());
            }
            (Some(old), copy) => {
                let edited = copy.is_none_or(|e| sha256::hex(e.body.as_bytes()) != e.hash);
                if req.diff {
                    diffs += 1;
                    diff(&shown, &old, &text)?;
                } else if req.force {
                    write(&path, &text)?;
                    wrote.push(name.clone());
                } else {
                    let why = match copy {
                        None => "isn't a copy gpui add made".to_string(),
                        Some(_) if edited => "has your edits".to_string(),
                        Some(_) => "is unedited, from another gpui-core".to_string(),
                    };
                    kept.push(format!("{} {why}", shown.display()));
                }
            }
        }
    }

    if req.diff {
        if diffs == 0 {
            println!("No differences from {from}.");
        }
        return Ok(());
    }

    let mut in_dir: BTreeSet<String> = have.keys().cloned().collect();
    in_dir.extend(wrote.iter().cloned());
    in_dir.extend(same.iter().cloned());
    let module = module(&in_dir, &registry);
    if std::fs::read_to_string(&mod_path).ok().as_deref() != Some(module.as_str()) {
        write(&mod_path, &module)?;
    }

    let list = |v: &[String]| {
        v.iter()
            .map(|n| n.replace('_', "-"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !wrote.is_empty() {
        println!(
            "Copied {} from {from} into {}.",
            list(&wrote),
            req.dir_shown.display()
        );
    }
    if !same.is_empty() {
        println!("Up to date: {}.", list(&same));
    }
    if !kept.is_empty() {
        println!("Left alone (--diff shows how they differ, --force replaces them):");
        for k in &kept {
            println!("  {k}");
        }
    }
    advise(root, req, &in_dir, &registry);
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// `diff -u` of a copy against what add would write.
fn diff(shown: &Path, old: &str, new: &str) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("gpui-add-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let (a, b) = (tmp.join("a"), tmp.join("b"));
    std::fs::write(&a, old).map_err(|e| e.to_string())?;
    std::fs::write(&b, new).map_err(|e| e.to_string())?;
    let label = shown.display().to_string();
    let status = Command::new("diff")
        .args(["-u", "--label", &label, "--label", &label])
        .arg(&a)
        .arg(&b)
        .status();
    let _ = std::fs::remove_dir_all(&tmp);
    match status {
        Ok(s) if s.code().is_some_and(|c| c <= 1) => Ok(()),
        Ok(s) => Err(format!("diff failed ({s})")),
        Err(e) => Err(format!("running diff: {e}")),
    }
}

/// The copies' mod.rs: their modules, what gpui-core's root exports from them, and `init`.
fn module(copies: &BTreeSet<String>, registry: &Registry) -> String {
    let mut out = format!(
        "{MOD_MARK} It's generated: add rewrites
//! it from the copies in this directory (the copies are yours to change; this file isn't).
//!
//! Call [`init`] once at startup: it runs gpui-core's `init` and binds the copies' keys.

// A copied component's whole API needn't be used.
#![allow(dead_code)]

use gpui::{{App, Global}};

"
    );
    for name in copies {
        out += &format!("pub mod {name};\n");
    }
    out += "\n";
    for name in copies {
        if let Some(c) = registry.components.get(name) {
            match c.exports.as_slice() {
                [] => {}
                [one] => out += &format!("pub use {name}::{one};\n"),
                many => out += &format!("pub use {name}::{{{}}};\n", many.join(", ")),
            }
        }
    }
    out += "
/// Sets the copies up, and gpui-core with them: gpui-core's theme if the host has set none,
/// and every copy's key bindings. Running it again does nothing.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    cx.set_global(Initialized);
    gpui_core::init(cx);
";
    for name in copies {
        if let Some(Component { init: Some(f), .. }) = registry.components.get(name) {
            out += &format!("    {name}::{f}(cx);\n");
        }
    }
    out += "}

/// Runs [`init`] after this frame if it hasn't run, as `gpui_core::base::init_soon` does: a
/// copy calls it when drawn, so its keys work even if the host never called `init`.
pub(crate) fn init_soon(cx: &mut App) {
    if !cx.has_global::<Initialized>() {
        cx.defer(init);
    }
}

struct Initialized;

impl Global for Initialized {}
";
    out
}

/// What the host still has to do: the crates the copies use, `mod ui;`, `ui::init`.
fn advise(root: &Path, req: &Request, copies: &BTreeSet<String>, registry: &Registry) {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let has = |dep: &str| {
        manifest.lines().any(|l| {
            let l = l.trim();
            l.starts_with(&format!("{dep} ="))
                || l.starts_with(&format!("{dep}="))
                || l == format!("[dependencies.{dep}]")
        })
    };
    let mut crates: BTreeSet<&str> = BTreeSet::new();
    for name in copies {
        if let Some(c) = registry.components.get(name) {
            crates.extend(c.crates.iter().map(String::as_str));
        }
    }
    let missing: Vec<&str> = ["gpui", "gpui-core"]
        .into_iter()
        .chain(crates)
        .filter(|d| !has(d))
        .collect();
    if !missing.is_empty() {
        println!(
            "The copies use {}, which Cargo.toml doesn't list: add {} (gpui-core's Cargo.toml has \
             the versions it builds with).",
            missing.join(", "),
            if missing.len() == 1 { "it" } else { "them" }
        );
    }
    let module_name = req
        .dir
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "ui".into());
    let declared = ["src/lib.rs", "src/main.rs"].iter().any(|f| {
        std::fs::read_to_string(root.join(f)).is_ok_and(|t| {
            t.contains(&format!("mod {module_name};"))
                || t.contains(&format!("mod {module_name} {{"))
        })
    });
    if !declared && req.dir_shown == Path::new("src").join(&module_name) {
        println!(
            "Then add `mod {module_name};` to src/lib.rs or src/main.rs, and call \
             `{module_name}::init(cx)` at startup."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_reads_back() {
        let dir = std::env::temp_dir().join(format!("gpui-cli-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let body = "use gpui::App;\n";
        let text = header("date_picker", "a/b@abc", "src/date_picker.rs", body) + body;
        std::fs::write(dir.join("date_picker.rs"), &text).unwrap();
        let (name, e) = existing(&dir.join("date_picker.rs")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(name, "date_picker");
        assert_eq!(e.body, body);
        assert_eq!(e.hash, sha256::hex(body.as_bytes()));
    }

    #[test]
    fn a_copy_made_before_the_rename_is_still_one() {
        let dir = std::env::temp_dir().join(format!("gpui-cli-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let body = "use gpui::App;\n";
        let new = header("select", "a/b@abc", "src/select.rs", body);
        let old = new.replacen(MARK, OLD_MARK, 1);
        std::fs::write(dir.join("select.rs"), old + body).unwrap();
        let (name, e) = existing(&dir.join("select.rs")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!((name.as_str(), e.body.as_str()), ("select", body));
    }
}
