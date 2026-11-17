//! Avatar and AvatarGroup: an avatar is an image named for its person to assistive technology,
//! in shadcn's three sizes; with no picture it draws its initials on a muted circle, and with
//! one (bytes the host hands it) it draws the picture; a group overlaps its avatars by eight
//! pixels and counts the rest past its `max`; a narrow space clips a group at its edge; and
//! nothing redraws while they sit there.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Element as _, Image, ImageFormat, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, RenderOnce as _, Role, Styled as _, TestAppContext,
    VisualTestContext, Window, accesskit, div, px,
};
use gpui_core::{Avatar, AvatarGroup, Root, Sizable as _};

/// A 4 by 4 red PNG.
const PNG: [u8; 75] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 6, 0,
    0, 0, 169, 241, 158, 126, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31,
    25, 51, 144, 46, 0, 0, 60, 64, 31, 225, 224, 129, 119, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174,
    66, 96, 130,
];

struct Host {
    width: f32,
    draws: Rc<Cell<usize>>,
    photo: Arc<Image>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let tagged =
            |tag: &'static str, a: Avatar| div().flex().debug_selector(move || tag.into()).child(a);
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(tagged("small", Avatar::new("Ada Lovelace").small()))
                    .child(tagged("medium", Avatar::new("Grace Hopper")))
                    .child(tagged("large", Avatar::new("Alan Turing").large()))
                    .child(tagged(
                        "photo",
                        Avatar::new("Edsger Dijkstra").image(self.photo.clone()),
                    )),
            )
            .child(
                div()
                    .flex()
                    .w(px(self.width))
                    .debug_selector(|| "group".into())
                    .child(
                        AvatarGroup::new("team")
                            .max(3)
                            .children(["A A", "B B", "C C", "D D", "E E"].map(Avatar::new)),
                    ),
            )
    }
}

fn open(cx: &mut TestAppContext, width: f32) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            width,
            draws: d,
            photo: Arc::new(Image::from_bytes(ImageFormat::Png, PNG.to_vec())),
        });
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    (draws, vcx)
}

#[gpui::test]
fn it_is_an_image_named_for_its_person(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 400.);
    let (role, label) = vcx.update(|window, cx| {
        let el = Avatar::new("Ada Lovelace")
            .render(window, cx)
            .into_element();
        let mut node = accesskit::Node::new(Role::Unknown);
        el.write_a11y_info(&mut node);
        (el.a11y_role(), node.label().map(str::to_owned))
    });
    assert_eq!(role, Some(Role::Image));
    assert_eq!(label.as_deref(), Some("Ada Lovelace"));
}

#[gpui::test]
fn it_comes_in_shadcns_sizes(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 400.);
    for (which, side) in [("small", 24.), ("medium", 32.), ("large", 40.)] {
        let b = vcx.debug_bounds(which).unwrap();
        assert_eq!(
            (b.size.width, b.size.height),
            (px(side), px(side)),
            "{which}"
        );
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_they_sit_there(cx: &mut TestAppContext) {
    let (draws, vcx) = open(cx, 400.);
    // The picture is decoded off the main thread, then drawn: after that, quiet.
    for _ in 0..3 {
        frame(vcx);
    }
    let settled = draws.get();
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame");
    }
    assert_eq!(draws.get(), settled);
}

#[cfg(target_os = "macos")]
mod drawn {
    use super::*;
    use gpui::{Bounds, Hsla, point};

    /// What was drawn: muted circles (their bounds in window pixels, as shown inside their
    /// clip), glyphs, and pictures, each with its centre.
    struct Drawn {
        circles: Vec<Bounds<f32>>,
        glyphs: Vec<Bounds<f32>>,
        pictures: Vec<Bounds<f32>>,
    }

    fn drawn(vcx: &mut VisualTestContext) -> Drawn {
        let muted: Hsla = gpui_core::Theme::dark().muted;
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            let scene = window.rendered_scene();
            let shown = |b: gpui::Bounds<gpui::ScaledPixels>,
                         m: gpui::Bounds<gpui::ScaledPixels>| {
                let f = |b: gpui::Bounds<gpui::ScaledPixels>| Bounds {
                    origin: point(b.origin.x.0 / k, b.origin.y.0 / k),
                    size: gpui::size(b.size.width.0 / k, b.size.height.0 / k),
                };
                f(b).intersect(&f(m))
            };
            Drawn {
                circles: scene
                    .quads
                    .iter()
                    .filter(|q| q.background.as_solid() == Some(muted))
                    .map(|q| shown(q.bounds, q.content_mask.bounds))
                    .filter(|b| b.size.width > 0.)
                    .collect(),
                glyphs: scene
                    .monochrome_sprites
                    .iter()
                    .map(|g| shown(g.bounds, g.content_mask.bounds))
                    .chain(
                        scene
                            .subpixel_sprites
                            .iter()
                            .map(|g| shown(g.bounds, g.content_mask.bounds)),
                    )
                    .filter(|b| b.size.width > 0.)
                    .collect(),
                pictures: scene
                    .polychrome_sprites
                    .iter()
                    .map(|g| shown(g.bounds, g.content_mask.bounds))
                    .filter(|b| b.size.width > 0.)
                    .collect(),
            }
        })
    }

    fn within(b: &Bounds<f32>, of: gpui::Bounds<gpui::Pixels>) -> bool {
        of.contains(&point(px(b.center().x), px(b.center().y)))
    }

    fn app() -> TestAppContext {
        TestAppContext::build_with_text_system(
            gpui::TestDispatcher::new(0),
            None,
            Arc::new(gpui_apple::AppleTextSystem::new()),
        )
    }

    #[gpui::test]
    fn without_a_picture_it_draws_initials_on_a_muted_circle() {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, 400.);
        let medium = vcx.debug_bounds("medium").unwrap();
        let d = drawn(vcx);
        assert!(d.circles.iter().any(|c| within(c, medium)), "no circle");
        assert_eq!(
            d.glyphs.iter().filter(|g| within(g, medium)).count(),
            2,
            "not two initials"
        );
        assert!(!d.pictures.iter().any(|p| within(p, medium)));
    }

    #[gpui::test]
    fn with_a_picture_it_draws_the_picture() {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, 400.);
        for _ in 0..3 {
            frame(vcx);
        }
        let photo = vcx.debug_bounds("photo").unwrap();
        let d = drawn(vcx);
        let pictures: Vec<_> = d.pictures.iter().filter(|p| within(p, photo)).collect();
        assert_eq!(pictures.len(), 1, "no picture: {:?}", d.pictures);
        assert!((pictures[0].size.width - 32.).abs() < 0.5, "{pictures:?}");
        assert!(
            !d.glyphs.iter().any(|g| within(g, photo)),
            "initials over it"
        );
    }

    #[gpui::test]
    fn a_group_overlaps_its_avatars_and_counts_the_rest() {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, 400.);
        let group = vcx.debug_bounds("group").unwrap();
        let mut circles: Vec<_> = drawn(vcx)
            .circles
            .into_iter()
            .filter(|c| within(c, group))
            .collect();
        circles.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
        // Three avatars' initials and the "+2", each a 32-pixel circle 24 pixels on.
        assert_eq!(circles.len(), 4, "{circles:?}");
        for w in circles.windows(2) {
            assert!(
                (w[1].origin.x - w[0].origin.x - 24.).abs() < 0.5,
                "{circles:?}"
            );
        }
    }

    #[gpui::test]
    fn a_narrow_space_clips_the_group_at_its_edge() {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, 50.);
        let group = vcx.debug_bounds("group").unwrap();
        let d = drawn(vcx);
        let right = f32::from(group.right()) + 2.5;
        for b in d.circles.iter().chain(&d.glyphs) {
            if b.origin.y >= f32::from(group.top()) - 3. {
                assert!(b.origin.x + b.size.width <= right, "{b:?} past {group:?}");
            }
        }
    }
}
