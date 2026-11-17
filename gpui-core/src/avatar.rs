//! Avatars, as shadcn draws them: a person's picture in a circle, or their initials on a muted
//! circle when there's no picture (or until it's decoded), in three sizes; and a group of
//! them overlapping, each ringed in the page's colour, with a "+N" for the rest.
//!
//! The picture is one the host already has, decoded or as bytes: an avatar never fetches
//! anything (a URL in a message is the host's to load, or not).
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use gpui::*;
//! # use gpui_core::{Avatar, AvatarGroup, Sizable as _};
//! # fn people(photo: Arc<RenderImage>) -> impl IntoElement {
//! div()
//!     .flex()
//!     .gap_4()
//!     .child(Avatar::new("Ada Lovelace").image(photo))
//!     .child(Avatar::new("Grace Hopper").large())
//!     .child(
//!         AvatarGroup::new("team")
//!             .max(3)
//!             .children(["Ada", "Grace", "Alan", "Edsger"].map(Avatar::new)),
//!     )
//! # }
//! ```

use std::sync::Arc;

use gpui::{
    AnyElement, App, BoxShadow, ElementId, Hsla, Image, ImageSource, InteractiveElement as _,
    IntoElement, ObjectFit, ParentElement as _, Refineable as _, Rems, RenderImage, RenderOnce,
    Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, StyledImage as _,
    Window, div, img, point, prelude::FluentBuilder as _, px, relative, rems,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::icon::{Icon, IconName};
use crate::size::{ControlSize, Sizable};
use crate::theme::ActiveTheme as _;

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// An avatar's side: shadcn's 24, 32 and 40 pixels, and 20 for the extra small size.
fn side(size: ControlSize) -> f32 {
    match size {
        ControlSize::XSmall => 20.,
        ControlSize::Small => 24.,
        ControlSize::Medium | ControlSize::Touch => 32.,
        ControlSize::Large => 40.,
    }
}

/// A picture the host already has: decoded ([`RenderImage`]) or as a file's bytes ([`Image`],
/// decoded off the main thread before it shows). Never a URL or a path.
#[derive(Clone)]
pub enum AvatarImage {
    Render(Arc<RenderImage>),
    Image(Arc<Image>),
}

impl From<Arc<RenderImage>> for AvatarImage {
    fn from(image: Arc<RenderImage>) -> Self {
        AvatarImage::Render(image)
    }
}

impl From<Arc<Image>> for AvatarImage {
    fn from(image: Arc<Image>) -> Self {
        AvatarImage::Image(image)
    }
}

impl From<AvatarImage> for ImageSource {
    fn from(image: AvatarImage) -> Self {
        match image {
            AvatarImage::Render(i) => ImageSource::Render(i),
            AvatarImage::Image(i) => ImageSource::Image(i),
        }
    }
}

/// The initials an avatar shows for `name`: the first letter of its first word and of its last
/// ("Ada Lovelace" is "AL", "Plato" "P"), in capitals, each a whole character as the reader
/// sees it (an accent, an emoji). None for a name with no words.
fn initials(name: &str) -> Option<SharedString> {
    let mut words = name.split_whitespace();
    let first = words.next()?;
    let mut out = String::new();
    let mut add = |w: &str| {
        if let Some(g) = w.graphemes(true).next() {
            out.extend(g.chars().flat_map(char::to_uppercase));
        }
    };
    add(first);
    if let Some(last) = words.next_back() {
        add(last);
    }
    Some(out.into())
}

/// A person's picture in a circle, or their initials when there's none. To assistive
/// technology it's an image named for the person.
#[derive(IntoElement)]
pub struct Avatar {
    id: Option<ElementId>,
    name: SharedString,
    image: Option<AvatarImage>,
    size: ControlSize,
    /// The ring a group draws around it, in the page's colour.
    ring: Option<Hsla>,
    style: StyleRefinement,
}

impl Avatar {
    /// An avatar for `name`, which assistive technology reads and its initials come from.
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            id: None,
            name: name.into(),
            image: None,
            size: ControlSize::Medium,
            ring: None,
            style: StyleRefinement::default(),
        }
    }

    /// The picture, which the host has already loaded. The initials show until it's decoded,
    /// and if it can't be.
    pub fn image(mut self, image: impl Into<AvatarImage>) -> Self {
        self.image = Some(image.into());
        self
    }

    /// Its id among its siblings (its name otherwise: give two avatars of one name ids).
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Sizable for Avatar {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Avatar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Two pixels of the page's colour around a circle, outside it (the web's `ring-2`).
fn ring(color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: false,
    }
}

/// What stands in for the picture: the initials (or a person's outline, with no name) on the
/// muted colour.
fn fallback(initials: Option<SharedString>, side: f32, bg: Hsla, fg: Hsla) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .whitespace_nowrap()
        .map(|d| match initials {
            Some(i) => d.child(i),
            None => d.child(Icon::new(IconName::User).size(css(side / 2.))),
        })
        .into_any_element()
}

impl RenderOnce for Avatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let side = side(self.size);
        let small = matches!(self.size, ControlSize::XSmall | ControlSize::Small);
        let (bg, fg) = (theme.muted, theme.muted_foreground);
        let initials = initials(&self.name);
        let id = self
            .id
            .unwrap_or_else(|| ElementId::Name(self.name.clone()));
        let mut el = div()
            .id(id)
            .role(Role::Image)
            .aria_label(self.name)
            .relative()
            .flex()
            .flex_none()
            .size(css(side))
            .rounded_full()
            .overflow_hidden()
            .text_size(if small { theme.text.xs } else { theme.text.sm })
            .line_height(relative(1.))
            .when_some(self.ring, |d, c| d.shadow(vec![ring(c)]));
        el.style().refine(&self.style);
        match self.image {
            None => el.child(fallback(initials, side, bg, fg)),
            Some(image) => {
                let (a, b) = (initials.clone(), initials);
                el.child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .with_loading(move || fallback(a.clone(), side, bg, fg))
                        .with_fallback(move || fallback(b.clone(), side, bg, fg)),
                )
            }
        }
    }
}

/// Avatars side by side, each overlapping the one before by eight pixels and ringed in the
/// page's colour, and at most [`max`](AvatarGroup::max) of them: the rest are a "+N" circle.
/// A size given to the group is every avatar's.
#[derive(IntoElement)]
pub struct AvatarGroup {
    id: ElementId,
    avatars: Vec<Avatar>,
    max: Option<usize>,
    size: Option<ControlSize>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl AvatarGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            avatars: Vec::new(),
            max: None,
            size: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn child(mut self, avatar: Avatar) -> Self {
        self.avatars.push(avatar);
        self
    }

    pub fn children(mut self, avatars: impl IntoIterator<Item = Avatar>) -> Self {
        self.avatars.extend(avatars);
        self
    }

    /// Shows at most this many avatars; the rest are counted in a "+N" circle after them.
    pub fn max(mut self, max: usize) -> Self {
        self.max = Some(max);
        self
    }

    /// What the group is to assistive technology ("Members").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Sizable for AvatarGroup {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Styled for AvatarGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AvatarGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let page = theme.background;
        let n = self.avatars.len();
        let shown = self.max.unwrap_or(n).min(n);
        let rest = n - shown;
        let size = self
            .size
            .or_else(|| self.avatars.first().map(|a| a.size))
            .unwrap_or_default();
        let s = side(size);
        let overlap = |i: usize| if i > 0 { css(-8.) } else { css(0.) };
        let avatars = self
            .avatars
            .into_iter()
            .take(shown)
            .enumerate()
            .map(|(i, mut a)| {
                a.size = size;
                a.ring = Some(page);
                a.ml(overlap(i))
            });
        let count = (rest > 0).then(|| {
            let small = matches!(size, ControlSize::XSmall | ControlSize::Small);
            div()
                .id("more")
                .role(Role::Label)
                .aria_label(format!("{rest} more"))
                .ml(overlap(shown))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(css(s))
                .rounded_full()
                .bg(theme.muted)
                .text_color(theme.muted_foreground)
                .text_size(if small { theme.text.xs } else { theme.text.sm })
                .line_height(relative(1.))
                .whitespace_nowrap()
                .shadow(vec![ring(page)])
                .child(format!("+{rest}"))
        });
        // In a narrow space the avatars past its edge are clipped there; the clip reaches two
        // pixels past the group each way so the rings aren't cut.
        let mut group = div()
            .id(self.id)
            .role(Role::Group)
            .when_some(self.label, |d, l| d.aria_label(l))
            .flex()
            .items_center()
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .p(css(2.))
            .m(css(-2.));
        group.style().refine(&self.style);
        group.children(avatars).children(count)
    }
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_are_the_first_and_last_words_first_letters() {
        let i = |n: &str| initials(n).map(|s| s.to_string());
        assert_eq!(i("Ada Lovelace").as_deref(), Some("AL"));
        assert_eq!(i("Plato").as_deref(), Some("P"));
        assert_eq!(i("  grace  brewster murray hopper ").as_deref(), Some("GH"));
        assert_eq!(i("élodie Ñúñez").as_deref(), Some("ÉÑ"));
        // A letter and its combining accent are one character to the reader.
        assert_eq!(i("e\u{301}mile zola").as_deref(), Some("E\u{301}Z"));
        assert_eq!(i("🦊 Fox").as_deref(), Some("🦊F"));
        assert_eq!(i("李 小龙").as_deref(), Some("李小"));
        assert_eq!(i("   "), None);
        assert_eq!(i(""), None);
    }
}
