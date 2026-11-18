//! `gpui add`: copies gpui-core's components into your crate, as shadcn/ui's CLI does for
//! React. You own the copies and change them as you like; they build on `gpui_core::base`,
//! which gpui-core keeps semver-stable, so gpui-core stays a dependency.
//!
//! ```text
//! gpui add select dialog      # into src/ui/, with what they need
//! gpui add --all
//! gpui add select --diff      # what a newer gpui-core changed in your copy
//! gpui list
//! ```

mod copy;
mod registry;
mod rewrite;
mod sha256;
mod source;

use std::path::PathBuf;
use std::process::ExitCode;

use source::Source;

const HELP: &str = "\
gpui: copies gpui-core's components into your crate, to own and change.

USAGE
    gpui add <component>... [options]   copy components (and what they need)
    gpui add --all [options]            copy every component
    gpui list [options]                 list the components

OPTIONS
    --dir <dir>          where the copies go, from your crate's root [default: src/ui]
    --namespace <name>   the copies' action namespace and key-context prefix: `ui` makes
                         `ui_select::Open` and `UiSelect` [default: ui]
    --rev <rev>          gpui-core's commit, tag or branch on GitHub
                         [default: the one your Cargo.lock has]
    --repo <owner/repo>  the GitHub repository [default: trevin-lee/gpui-essentials, or
                         your Cargo.lock's]
    --path <dir>         a local gpui-essentials checkout instead of GitHub
    --diff               show what would change in copies you have; write nothing
    --force              replace copies you have, your edits included
    -h, --help           this
    -V, --version        the version

Then add `mod ui;` to your lib.rs or main.rs and call `ui::init(cx)` at startup.";

struct Options {
    command: String,
    names: Vec<String>,
    all: bool,
    dir: PathBuf,
    namespace: String,
    rev: Option<String>,
    repo: Option<String>,
    path: Option<PathBuf>,
    diff: bool,
    force: bool,
}

fn parse(args: Vec<String>) -> Result<Option<Options>, String> {
    let mut o = Options {
        command: String::new(),
        names: Vec::new(),
        all: false,
        dir: PathBuf::from("src/ui"),
        namespace: "ui".into(),
        rev: None,
        repo: None,
        path: None,
        diff: false,
        force: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} needs a value (see --help)"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-V" | "--version" => {
                println!("gpui {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--all" => o.all = true,
            "--diff" => o.diff = true,
            "--force" => o.force = true,
            "--dir" => o.dir = value("--dir")?.into(),
            "--namespace" => o.namespace = value("--namespace")?,
            "--rev" => o.rev = Some(value("--rev")?),
            "--repo" => o.repo = Some(value("--repo")?),
            "--path" => o.path = Some(value("--path")?.into()),
            a if a.starts_with('-') => return Err(format!("unknown option {a} (see --help)")),
            a if o.command.is_empty() => o.command = a.to_string(),
            // `date-picker` and `date_picker` are the same component.
            a => o.names.push(a.replace('-', "_")),
        }
    }
    let valid = !o.namespace.is_empty()
        && o.namespace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && o.namespace.starts_with(|c: char| c.is_ascii_lowercase());
    if !valid {
        return Err(format!(
            "--namespace `{}` must be a lowercase identifier, like `ui` or `my_ui`",
            o.namespace
        ));
    }
    if o.namespace == "gpui_core" {
        return Err("--namespace can't be gpui-core's own `gpui_core`".into());
    }
    if o.diff && o.force {
        return Err("--diff writes nothing and --force overwrites: pick one".into());
    }
    if o.path.is_some() && (o.rev.is_some() || o.repo.is_some()) {
        return Err("--path reads a checkout: it takes no --rev or --repo".into());
    }
    Ok(Some(o))
}

fn source(o: &Options, root: &std::path::Path) -> Result<Source, String> {
    if let Some(path) = &o.path {
        return Ok(Source::Local(path.clone()));
    }
    match (&o.rev, &o.repo) {
        (Some(rev), repo) => Ok(Source::GitHub {
            repo: repo.clone().unwrap_or_else(|| source::DEFAULT_REPO.into()),
            rev: rev.clone(),
        }),
        (None, repo) => {
            let mut locked = source::locked(root)?;
            if let (Source::GitHub { repo: r, .. }, Some(repo)) = (&mut locked, repo) {
                *r = repo.clone();
            }
            Ok(locked)
        }
    }
}

fn run(o: Options) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    match o.command.as_str() {
        "list" => {
            let root = source::crate_root(&cwd).unwrap_or(cwd);
            let source = source(&o, &root)?;
            let registry = registry::Registry::parse(&source.read("components.toml")?)?;
            let width = registry
                .components
                .keys()
                .map(String::len)
                .max()
                .unwrap_or(0);
            for (name, c) in &registry.components {
                println!("{:width$}  {}", name.replace('_', "-"), c.about);
            }
            Ok(())
        }
        "add" => {
            if o.names.is_empty() && !o.all {
                return Err("add which? Name components (`gpui add select`), or --all; \
                            `gpui list` shows them"
                    .into());
            }
            let root = source::crate_root(&cwd)?;
            let source = source(&o, &root)?;
            copy::add(
                &copy::Request {
                    names: o.names,
                    all: o.all,
                    dir: root.join(&o.dir),
                    dir_shown: o.dir,
                    namespace: o.namespace,
                    diff: o.diff,
                    force: o.force,
                },
                &root,
                &source,
            )
        }
        "" => Err(HELP.into()),
        other => Err(format!(
            "unknown command `{other}`: it's `add` or `list` (see --help)"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match parse(args) {
        Ok(None) => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Some(o)) => run(o),
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gpui: {e}");
            ExitCode::FAILURE
        }
    }
}
