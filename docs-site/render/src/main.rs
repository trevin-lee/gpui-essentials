//! Renders the documentation site: every variant of every component in the registry, drawn
//! offscreen in the light and the dark theme, and a page per component showing the pictures
//! beside the code that drew them.
//!
//! ```sh
//! cargo run --manifest-path docs-site/render/Cargo.toml -- --out $TMPDIR/site [--only button]
//! ```
//!
//! It writes `<out>/img/<component>/<variant>-{light,dark}.png`, `<out>/index.html`,
//! `<out>/components/<component>.html` and `<out>/style.css`. With `--only`, it draws that
//! component's pictures alone, and still writes every page.

mod html;
mod install;
mod registry;
mod shot;
mod site;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use gpui::AsyncApp;
use gpui_core::Icons;

use crate::install::Installs;
use crate::site::Component;

/// The repository's root, where the crates' folders are.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

struct Args {
    out: PathBuf,
    only: Option<String>,
}

fn args() -> Result<Args> {
    let mut out = None;
    let mut only = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().map(PathBuf::from),
            "--only" => only = it.next(),
            "-h" | "--help" => {
                println!("render --out <dir> [--only <component slug>]");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?} (render --out <dir> [--only <slug>])"),
        }
    }
    let out = out.context("--out <dir> is required")?;
    Ok(Args { out, only })
}

fn main() -> ExitCode {
    let args = match args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e:#}");
            return ExitCode::from(2);
        }
    };
    let components = registry::components();
    if let Some(only) = &args.only
        && !components.iter().any(|c| c.slug == only)
    {
        eprintln!("no component {only:?} in the registry");
        return ExitCode::from(2);
    }
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            cx.spawn(async move |cx| {
                if let Err(e) = render_site(&args, &components, cx).await {
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

async fn render_site(args: &Args, components: &[Component], cx: &mut AsyncApp) -> Result<()> {
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
    let installs = Installs::load(&Path::new(ROOT).join("gpui-core/components.toml"))?;
    html::write_site(&args.out, components, &installs)?;
    println!(
        "{pictures} pictures and {} pages in {} ({:.1} s)",
        components.len() + 1,
        args.out.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
