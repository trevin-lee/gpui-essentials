//! What the site shows: families of components, each with its variants and their demos.

use gpui::{AnyElement, AnyView, App, Window};

/// The groups the sidebar and the index list components under, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    Forms,
    Overlays,
    Menus,
    Charts,
    Scrolling,
    Navigation,
    Containers,
    Display,
}

impl Family {
    pub const ALL: [Family; 8] = [
        Family::Forms,
        Family::Overlays,
        Family::Menus,
        Family::Charts,
        Family::Scrolling,
        Family::Navigation,
        Family::Containers,
        Family::Display,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Family::Forms => "Forms",
            Family::Overlays => "Overlays",
            Family::Menus => "Menus",
            Family::Charts => "Charts",
            Family::Scrolling => "Scrolling",
            Family::Navigation => "Navigation",
            Family::Containers => "Containers",
            Family::Display => "Display",
        }
    }
}

/// One component's page.
pub struct Component {
    pub family: Family,
    pub name: &'static str,
    /// Its page's file name and its pictures' folder: `components/<slug>.html`, `img/<slug>/`.
    pub slug: &'static str,
    /// One line under its name, and on its card in the index.
    pub about: &'static str,
    /// Its rustdoc page, relative to the API docs' root (`gpui_core/struct.Button.html`).
    pub docs: &'static str,
    pub variants: Vec<Variant>,
}

/// One picture on a component's page, and the code that drew it.
pub struct Variant {
    pub name: &'static str,
    /// Its pictures' file names: `<slug>-light.png` and `<slug>-dark.png`.
    pub slug: &'static str,
    /// The demo file's text, shown beside the picture.
    pub code: &'static str,
    pub demo: Demo,
    /// What the pointer does to the demo before its picture is taken.
    pub act: Act,
}

/// What the pointer does before a picture is taken, in the middle of what the demo drew: how a
/// picture shows a menu, a hover card or a tooltip open, which keep whether they're open to
/// themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Nothing: the demo as it first draws.
    None,
    /// A press with the main button.
    Click,
    /// A press with the secondary button (a context menu).
    RightClick,
    /// The pointer rests there (a hover card, a tooltip).
    Hover,
}

/// How a demo draws.
#[derive(Clone, Copy)]
pub enum Demo {
    /// An element, built again each frame: the demo file is an expression.
    Element(fn(&mut Window, &mut App) -> AnyElement),
    /// A view with state of its own, built once: the demo file defines `Example`, a `Render`.
    View(fn(&mut Window, &mut App) -> AnyView),
}

impl Demo {
    pub fn is_view(self) -> bool {
        matches!(self, Demo::View(_))
    }
}
