//! Every component on the site, and its variants. To add a variant, write its demo file and
//! add one line to its component's `variants`; to add a component, add a `component!` block.
//! See ../../README.md.
//!
//! A variant is `demo("Name", "slug")` or `view("Name", "slug")`, drawn from
//! `docs-site/demos/<component slug>/<variant slug>.rs`:
//!
//! - `demo`: the file is one expression, of a type that is `IntoElement`, with
//!   `window: &mut Window` and `cx: &mut App` in scope.
//! - `view`: the file defines `pub struct Example`, with
//!   `pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self` and `impl Render`, for a
//!   demo with state of its own (an `InputState`, a toggle that turns).
//!
//! Both are compiled with `use gpui::*; use gpui_core::*;` and nothing else, so they use only
//! what those crates export.

use gpui::{AnyElement, AnyView, App, AppContext as _, Window};

use crate::site::{Component, Demo, Family, Variant};

/// A demo file's path, from this crate's manifest.
macro_rules! demo_path {
    ($component:literal, $variant:literal) => {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../demos/",
            $component,
            "/",
            $variant,
            ".rs"
        )
    };
}

/// One variant of the component `$component` (its slug).
macro_rules! variant {
    (@demo $component:literal, $name:literal, $slug:literal) => {
        Variant {
            name: $name,
            slug: $slug,
            code: include_str!(demo_path!($component, $slug)),
            // The file wrapped in `fn render(window, cx)` by build.rs.
            demo: Demo::Element(include!(concat!(
                env!("OUT_DIR"),
                "/demos/",
                $component,
                "/",
                $slug,
                ".rs"
            ))),
        }
    };
    (@view $component:literal, $name:literal, $slug:literal) => {
        Variant {
            name: $name,
            slug: $slug,
            code: include_str!(demo_path!($component, $slug)),
            demo: Demo::View({
                mod demo {
                    #[allow(unused_imports)]
                    use gpui::*;
                    #[allow(unused_imports)]
                    use gpui_core::*;
                    include!(demo_path!($component, $slug));
                }
                fn build(window: &mut Window, cx: &mut App) -> AnyView {
                    cx.new(|cx| demo::Example::new(window, cx)).into()
                }
                build
            }),
        }
    };
}

/// A component and its variants, in the order they show on its page.
macro_rules! component {
    (
        family: $family:ident,
        name: $name:literal,
        slug: $slug:literal,
        about: $about:literal,
        docs: $docs:literal,
        variants: [$($kind:ident($vname:literal, $vslug:literal)),* $(,)?] $(,)?
    ) => {
        Component {
            family: Family::$family,
            name: $name,
            slug: $slug,
            about: $about,
            docs: $docs,
            variants: vec![$(variant!(@$kind $slug, $vname, $vslug)),*],
        }
    };
}

/// The site's components. The index and the sidebar sort them by family, then by name.
pub fn components() -> Vec<Component> {
    vec![
        component! {
            family: Forms,
            name: "Button",
            slug: "button",
            about: "Starts an action: a label, an icon or both, in six variants and five sizes.",
            docs: "gpui_core/struct.Button.html",
            variants: [
                demo("Default", "default"),
                demo("Secondary", "secondary"),
                demo("Outline", "outline"),
                demo("Ghost", "ghost"),
                demo("Destructive", "destructive"),
                demo("Link", "link"),
                demo("Sizes", "sizes"),
                demo("With an icon", "with-icon"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Checkbox",
            slug: "checkbox",
            about: "A box to check, with its label: checked, unchecked or partly (a dash).",
            docs: "gpui_core/struct.Checkbox.html",
            variants: [
                demo("Default", "default"),
                view("Select all", "select-all"),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Switch",
            slug: "switch",
            about: "An on/off switch whose thumb slides between the two.",
            docs: "gpui_core/struct.Switch.html",
            variants: [
                demo("Default", "default"),
                demo("With a label", "with-label"),
                demo("Disabled", "disabled"),
                view("Interactive", "interactive"),
            ],
        },
        component! {
            family: Containers,
            name: "Card",
            slug: "card",
            about: "A raised panel with a header, content and a footer, each drawn only when given.",
            docs: "gpui_core/struct.Card.html",
            variants: [
                demo("Default", "default"),
                demo("Small", "small"),
                demo("Content only", "content-only"),
            ],
        },
        component! {
            family: Display,
            name: "Badge",
            slug: "badge",
            about: "A small pill of text, perhaps with an icon, in four looks.",
            docs: "gpui_core/struct.Badge.html",
            variants: [
                demo("Default", "default"),
                demo("Secondary", "secondary"),
                demo("Destructive", "destructive"),
                demo("Outline", "outline"),
                demo("With an icon", "with-icon"),
            ],
        },
    ]
}
