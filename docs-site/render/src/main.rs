//! Renders the documentation site: every variant of every component in the registry, drawn
//! offscreen in the light and the dark theme, and a page per component showing the pictures
//! beside the code that drew them.
//!
//! ```sh
//! cargo run --manifest-path docs-site/render/Cargo.toml -- --out $TMPDIR/site [--only button]
//! ```
//!
//! It writes `<out>/img/<component>/<variant>-{light,dark}.png`, the crates' pictures in
//! `<out>/img/crates/<crate>/`, `<out>/index.html`, `<out>/components/<component>.html`,
//! `<out>/crates/<crate>.html` and `<out>/style.css`. With `--only`, it makes that component's
//! or crate's pictures alone, and still writes every page.
//!
//! `render --json <file>` writes the registry as JSON for the Next.js site (docs-site/web)
//! instead, and draws nothing.

mod html;
mod install;
mod json;
mod shot;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use gpui::AsyncApp;
use gpui_core::Icons;

use crate::install::Installs;
use docs_site_registry::{Component, Crate};

/// The repository's root, where the crates' folders are.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

struct Args {
    out: PathBuf,
    only: Option<String>,
}

/// What to do: draw the site, or write the registry as JSON.
enum Command {
    Render(Args),
    Json(PathBuf),
}

const USAGE: &str = "render --out <dir> [--only <component slug>] | render --json <file>";

fn args() -> Result<Command> {
    let mut out = None;
    let mut only = None;
    let mut json = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().map(PathBuf::from),
            "--only" => only = it.next(),
            "--json" => json = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?} ({USAGE})"),
        }
    }
    if let Some(json) = json {
        return Ok(Command::Json(json));
    }
    let out = out.with_context(|| format!("--out <dir> or --json <file> is required ({USAGE})"))?;
    Ok(Command::Render(Args { out, only }))
}

fn main() -> ExitCode {
    let args = match args() {
        Ok(Command::Render(args)) => args,
        Ok(Command::Json(path)) => return write_json(&path),
        Err(e) => {
            eprintln!("{e:#}");
            return ExitCode::from(2);
        }
    };
    let components = docs_site_registry::components();
    let crates = docs_site_registry::crates();
    if let Some(only) = &args.only
        && !components.iter().any(|c| c.slug == only)
        && !crates.iter().any(|c| c.slug == only)
    {
        eprintln!("no component or crate {only:?} in the registry");
        return ExitCode::from(2);
    }
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            cx.spawn(async move |cx| {
                if let Err(e) = render_site(&args, &components, &crates, cx).await {
                    eprintln!("{e:#}");
                    // The app's run loop may end the process itself on quitting.
                    std::process::exit(1);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    ExitCode::SUCCESS
}

fn write_json(path: &Path) -> ExitCode {
    let written =
        Installs::load(&Path::new(ROOT).join("gpui-core/components.toml")).and_then(|installs| {
            json::write(
                path,
                &docs_site_registry::components(),
                &docs_site_registry::crates(),
                &installs,
            )
        });
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::from(1)
        }
    }
}

async fn render_site(
    args: &Args,
    components: &[Component],
    crates: &[Crate],
    cx: &mut AsyncApp,
) -> Result<()> {
    let started = Instant::now();
    let mut pictures = 0;
    for c in components {
        if args.only.as_ref().is_some_and(|only| only != c.slug) {
            continue;
        }
        let dir = args.out.join("img").join(c.slug);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        for v in &c.variants {
            let what = format!("{}/{}", c.slug, v.slug);
            let light = shot::capture(v.demo, v.act, false, cx)
                .await
                .with_context(|| format!("drawing {what} (light)"))?;
            let dark = shot::capture(v.demo, v.act, true, cx)
                .await
                .with_context(|| format!("drawing {what} (dark)"))?;
            let [light, dark] = shot::crop_pair(light, dark, &what)?;
            shot::save_png(&light, &dir.join(format!("{}-light.png", v.slug)))?;
            shot::save_png(&dark, &dir.join(format!("{}-dark.png", v.slug)))?;
            println!("{what}: {}×{}", light.width(), light.height());
            pictures += 2;
        }
    }
    for c in crates {
        if args.only.as_ref().is_some_and(|only| only != c.slug) {
            continue;
        }
        let dir = args.out.join("img/crates").join(c.slug);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        for (file, _) in &c.pictures {
            let from = Path::new(ROOT).join(c.dir).join("docs").join(file);
            shot::copy_picture(&from, &dir.join(file))?;
            pictures += 1;
        }
        println!("{}: {} pictures", c.name, c.pictures.len());
    }
    let installs = Installs::load(&Path::new(ROOT).join("gpui-core/components.toml"))?;
    html::write_site(&args.out, components, crates, &installs)?;
    println!(
        "{pictures} pictures and {} pages in {} ({:.1} s)",
        components.len() + crates.len() + 1,
        args.out.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
