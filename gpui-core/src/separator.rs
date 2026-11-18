//! A separator, as shadcn draws it (the web app's `separator.tsx`): a one-pixel line in the
//! border colour, across its parent's width or down its height.
//!
//! It's decorative unless told otherwise: drawn, and nothing to assistive technology. One that
//! divides content a reader should know is divided (two groups in a menu, a toolbar's halves)
//! says so with [`decorative(false)`](Separator::decorative): it's then a separator, with its
//! orientation, to assistive technology.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Separator;
//! # fn sections() -> impl IntoElement {
//! div()
//!     .flex()
//!     .flex_col()
//!     .child("Profile")
//!     .child(Separator::horizontal())
//!     .child(div().flex().h_5().child("Blog").child(Separator::vertical()).child("Docs"))
//! # }
//! ```

use gpui::{
    App, ElementId, InteractiveElement as _, IntoElement, Orientation, Refineable as _, RenderOnce,
    Role, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::base::ActiveTheme as _;

/// A one-pixel line: across ([`horizontal`](Separator::horizontal)) or down
/// ([`vertical`](Separator::vertical)). A horizontal one takes its parent's width; a vertical
/// one stretches to the height of the flex row it's in.
#[derive(IntoElement)]
pub struct Separator {
    id: ElementId,
    vertical: bool,
    decorative: bool,
    style: StyleRefinement,
}

impl Separator {
    pub fn horizontal() -> Self {
        Self {
            id: ElementId::Name("separator".into()),
            vertical: false,
            decorative: true,
            style: StyleRefinement::default(),
        }
    }

    pub fn vertical() -> Self {
        Self {
            vertical: true,
            ..Self::horizontal()
        }
    }

    /// Whether it's only drawn (the default) or also a separator to assistive technology.
    /// Give a separator that isn't decorative an [`id`](Separator::id) of its own if there are
    /// others like it beside it.
    pub fn decorative(mut self, decorative: bool) -> Self {
        self.decorative = decorative;
        self
    }

    /// Its id among its siblings (it's `"separator"` otherwise).
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl Styled for Separator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Separator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut line = div()
            .id(self.id)
            .flex_none()
            .bg(cx.theme().border)
            .map(|d| {
                if self.vertical {
                    d.w(px(1.)).self_stretch()
                } else {
                    d.h(px(1.)).w_full()
                }
            })
            .when(!self.decorative, |d| {
                // AccessKit has no separator role of its own: ARIA's `separator` is its
                // Splitter (as in Chromium, which AccessKit's roles come from).
                d.role(Role::Splitter).aria_orientation(if self.vertical {
                    Orientation::Vertical
                } else {
                    Orientation::Horizontal
                })
            });
        line.style().refine(&self.style);
        line
    }
}
