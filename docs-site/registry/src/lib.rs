//! The docs site's registry: every component, its variants and the demos that draw them
//! (`docs-site/demos/<component>/<variant>.rs`, compiled in), and the crates' pages. The
//! renderer (`docs-site/render`) draws each demo's pictures from it, and the live bundle
//! (`docs-site/live`) runs each demo in a browser from it.

mod registry;
mod site;

pub use registry::{components, crates};
pub use site::{Act, Component, Crate, Demo, Family, Variant};
