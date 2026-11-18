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
            family: Forms,
            name: "Input",
            slug: "input",
            about: "A text field of one line or several, its text kept in an InputState.",
            docs: "gpui_core/struct.Input.html",
            variants: [
                view("Default", "default"),
                view("Password", "password"),
                view("Textarea", "textarea"),
            ],
        },
        component! {
            family: Forms,
            name: "Input Group",
            slug: "input-group",
            about: "A text field with icons, text or buttons inside its border, before or after.",
            docs: "gpui_core/struct.InputGroup.html",
            variants: [
                view("With an icon", "icon"),
                view("Text either side", "text"),
                view("With a button", "button"),
                view("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Input OTP",
            slug: "input-otp",
            about: "A one-time code, a box per character, filled by typing or pasting.",
            docs: "gpui_core/struct.InputOtp.html",
            variants: [
                view("Default", "default"),
                view("Letters and digits", "letters"),
                view("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Radio Group",
            slug: "radio-group",
            about: "One choice of several, each option a circle with its label.",
            docs: "gpui_core/struct.RadioGroup.html",
            variants: [
                view("Default", "default"),
                demo("Horizontal", "horizontal"),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Select",
            slug: "select",
            about: "A button that opens a list of options, to pick one.",
            docs: "gpui_core/struct.Select.html",
            variants: [
                view("Default", "default"),
                demo("Groups", "groups"),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Slider",
            slug: "slider",
            about: "A value along a track, set by dragging or with the arrow keys.",
            docs: "gpui_core/struct.Slider.html",
            variants: [
                view("Default", "default"),
                view("With its value", "value"),
            ],
        },
        component! {
            family: Forms,
            name: "Toggle",
            slug: "toggle",
            about: "A button that stays pressed until pressed again.",
            docs: "gpui_core/struct.Toggle.html",
            variants: [
                view("Default", "default"),
                demo("Outline", "outline"),
                demo("Sizes", "sizes"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Toggle Group",
            slug: "toggle-group",
            about: "Toggles in a row: at most one on, or several.",
            docs: "gpui_core/struct.ToggleGroup.html",
            variants: [
                view("One at most", "single"),
                demo("Several", "multiple"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Segmented Control",
            slug: "segmented-control",
            about: "One of a few choices, as segments joined in a row.",
            docs: "gpui_core/struct.SegmentedControl.html",
            variants: [
                view("Default", "default"),
                demo("With icons", "icons"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Date Picker",
            slug: "date-picker",
            about: "A day, or a range of days, picked from a month.",
            docs: "gpui_core/struct.DatePicker.html",
            variants: [
                view("Default", "default"),
                view("Month", "month"),
            ],
        },
        component! {
            family: Forms,
            name: "Field",
            slug: "field",
            about: "A control with its label, a line of help or an error, and sets of them.",
            docs: "gpui_core/struct.Field.html",
            variants: [
                view("Default", "default"),
                view("With an error", "error"),
                view("Field set", "fieldset"),
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
