# The documentation site

A static site with a page per component: every variant drawn in the light and the dark theme,
the code that drew it, and a link to its API docs. It is published at
<https://trevin-lee.github.io/gpui-essentials>. A second site, in the manner of ui.shadcn.com,
runs the components live in the browser: see [The Next.js site](#the-nextjs-site).

- `demos/<component>/<variant>.rs` are the demos: each file is compiled into the renderer and
  the live bundle, and shown on the page, so the code on the page is always the code that drew
  the picture and that runs live.
- `registry/` lists the components and their variants (`registry/src/registry.rs`), with the
  demos compiled in: a library the renderer and the live bundle share.
- `render/` is a Rust program that draws each demo in a hidden window (gpui-core's `shots`
  feature, macOS only), saves the pictures and writes the HTML. It is its own Cargo workspace,
  with its own `Cargo.lock` (the registry is in it too), so it never changes the crates'
  workspace.
- `live/` is the live code, built for the browser: the Pages site's live page, and every demo
  for the Next.js site's previews.
- `web/` is the Next.js site.
- `static/style.css` is the Pages site's one stylesheet.

## Render it

On a Mac, from the repository's root:

```sh
cargo +1.100.0 run --manifest-path docs-site/render/Cargo.toml -- --out $TMPDIR/site
```

`--only <component slug>` draws one component's pictures (and still writes every page), which
is quicker while working on it:

```sh
cargo +1.100.0 run --manifest-path docs-site/render/Cargo.toml -- --out $TMPDIR/site --only button
```

It writes `index.html`, `components/<slug>.html`, `style.css` and
`img/<slug>/<variant>-{light,dark}.png`. Render outside the repository: the pictures are build
output and are never committed.

On a pull request that changes docs-site/, the `pages` workflow lints the renderer, renders the
site on GitHub's macOS runner and uploads it as the `site` artifact (`gh run download <run> -n
site`). Run it by hand from the Actions tab for any branch. Off macOS, `cargo clippy` still
compiles the renderer and every demo; only drawing the pictures needs a Mac.

## The live page

`live.html` runs a small app built from gpui-core's components in the browser
(`live/src/lib.rs`), on GPUI's web platform, single-threaded, so Pages needs no special
headers. Nothing downloads until the visitor presses Start: then about 3 MB (gzipped) of wasm.
It brings its own fonts (Inter, SIL OFL, cut down to Latin) and only the icons it draws.
The pages workflow builds it on Linux with `live/build.sh` (the wasm32 target, wasm-bindgen at
the version `live/Cargo.toml` pins, and binaryen's wasm-opt) into `live/` beside the pages.

## Publishing

On a push to main that changes docs-site/, every night, and when run by hand on main, the
`pages` workflow renders the site, adds every crate's rustdoc (`cargo doc --workspace
--no-deps`) as `api/`, and publishes it to GitHub Pages. Pages has to be on, with GitHub Actions
as its source (Settings, Pages).

## Preview it

```sh
python3 -m http.server -d $TMPDIR/site
```

and open <http://localhost:8000>. Every link is relative, so opening `index.html` from the
folder works too. The "Docs" links lead to `api/`, the rustdoc output, which the published site
adds beside the pages.

## Add a variant

1. Write `docs-site/demos/<component>/<variant>.rs`. For a component without state of its own,
   it's one expression whose type is `IntoElement`, written as the body of a `render`:

   ```rust
   // A quieter action beside the main one: filled with the secondary colour.
   Button::new("draft").label("Save draft").secondary()
   ```

   `use gpui::*;` and `use gpui_core::*;` are in scope, and so are `window: &mut Window` and
   `cx: &mut App`. Several pieces go in a `div()` (`.flex().gap(rems(0.5))…`). Start the file
   with a comment saying what the variant is for; it shows on the page.

2. Add a line to its component's `variants` in `registry/src/registry.rs`:

   ```rust
   demo("Secondary", "secondary"),
   ```

   The name is the heading on the page; the slug is the file's name and the pictures'.

A demo that needs state (an `InputState`, a switch that turns, a checked list) is a small view
instead. The file defines `pub struct Example`, `pub fn new(window: &mut Window, cx: &mut
Context<Self>) -> Self` and `impl Render for Example`; see `demos/switch/interactive.rs`. Its
registry line is `view("Interactive", "interactive")`.

Keep demos to what `gpui` and `gpui_core` export, never a crate's internals, and to made-up
data: names like Alex and Sam, addresses at example.com.

## Add a component

Add a `component!` block to `components()` in `registry/src/registry.rs`, and its demos under
`demos/<slug>/`:

```rust
component! {
    family: Display,
    name: "Badge",
    slug: "badge",
    about: "A small pill of text, perhaps with an icon, in four looks.",
    docs: "gpui_core/struct.Badge.html",
    variants: [
        demo("Default", "default"),
        demo("Outline", "outline"),
    ],
},
```

- `family` is one of `Forms`, `Overlays`, `Menus`, `Charts`, `Scrolling`, `Navigation`,
  `Containers` and `Display`: the sidebar's groups.
- `about` is one line, under the name and on the index's card.
- `docs` is the rustdoc page, from the API docs' root.
- Its Installation section comes from gpui-core's `components.toml`, found by the name in
  `docs`: `gpui add <name>` with what it also copies and the crates it needs, or, for
  what stays in gpui-core (inputs, popovers, charts…), the dependency alone.
- The first variant is the picture on the index's card.

Then render it with `--only <slug>` and look at both pictures.

## Add a crate's page

A crate's page shows pictures already in its `docs/` folder (the ones its README shows), with
links to its API docs and its README. Add a `Crate { … }` to `crates()` in
`registry/src/registry.rs`: its name, a slug for the page, its folder, its library's name, a
line about it and its pictures, each with what it shows (its README's alt text). The renderer
scales them to 1600 pixels wide at most and saves them in 256 colours, as it does the rest.

## How the pictures are made

Each variant is drawn in the middle of a hidden window 720 by 600 points, once in
`Theme::light()` and once in `Theme::dark()`, at the screen's scale (2× on a Retina Mac, so 1440
pixels wide; 1× on GitHub's macOS runners, so 720). Both pictures are cropped to the rows the content covers in either, with 40 points
above and below (at least 180 points tall), and saved as PNGs of at most 256 colours: the
picture's own colours when it has that few, otherwise a palette from NeuQuant
([color_quant](https://crates.io/crates/color_quant), MIT) with the background kept exact. The
renderer warns when a demo reaches the stage's edge, where it would be cut.

Popovers, menus and dialogs are drawn where they open, so a demo of one opens it (`open(true)`)
and leaves room for it.

## The Next.js site

`web/` is a Next.js site (App Router, Tailwind, a static export) in the manner of
ui.shadcn.com: a sidebar of families and components, and a page per component with a Preview
and a Code tab for each variant, its installation and a light and dark theme. The crates built
on gpui-core (the registry's `crates()`) each have a page too, with their pictures, their
installation and links to their README and API docs. It's hosted on
Vercel.

Its content comes from the registry, never copied by hand. It reads three things the Rust side
makes, none of them committed:

- `web/data/registry.json`: the registry as JSON (every component's name, family, line, API
  page, installation, and its variants' code), written by the renderer without drawing
  anything, so on any platform:

  ```sh
  cargo run --manifest-path docs-site/render/Cargo.toml -- --json docs-site/web/data/registry.json
  ```

- `web/public/img/`: the pictures, the renderer's `img/` folder (`--out`, macOS).
- `web/public/live/`: the live bundle, `live/build.sh docs-site/web/public/live`.

Then, in `web/`, `npm ci` and `npm run build` write the site to `web/out/`
(`npm run dev` serves it while you work).

The `pages` workflow's `web` job does all of this on Linux, with the pictures from the macOS
render, and deploys it with `vercel deploy --prebuilt` (after `web/scripts/vercel-output.sh`):
a preview for a pull request, production on main. It needs the repository's secrets
`VERCEL_TOKEN`, `VERCEL_ORG_ID` and `VERCEL_PROJECT_ID`; without them it builds the site and
stops.

### Live previews

Every preview has a Run live button (but those of the components in `PICTURES_ONLY`,
`web/lib/live.ts`, with the reason): it downloads the live bundle, once a visit, and runs that
variant's demo in GPUI, in place of its picture and where the picture was, with room below for
what it opens. After that, each page's first preview goes live by itself.

GPUI's web platform draws one window per page, in one canvas it adds to the page's body, and
can't open a second window or reopen one. So the site starts the app once (`stage` in
`live/src/stage.rs`), moves that canvas into whichever preview is live, and has the window
draw that preview's demo (`mount`); the others show their pictures. Every demo in the registry
is compiled into the bundle, so a new demo needs no wiring to run live.
