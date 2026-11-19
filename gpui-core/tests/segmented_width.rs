//! SegmentedControl's width: without one it's as wide as its labels, whole, even beside a
//! spacer that takes the rest of a row; given one, its options split it equally. Laid out with
//! the Mac's text system, so the labels have their real widths.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Context, IntoElement, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{Root, SegmentedControl};

/// Where the control sits: alone in a wide row, beside a spacer in a narrower one, or given a
/// width.
#[derive(Clone, Copy)]
enum Place {
    Alone,
    BesideSpacer,
    Given,
}

struct Host(Place);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let control = SegmentedControl::new("shelf")
            .accessibility_label("Shelf")
            .segment("favorites", "Favorites")
            .segment("recents", "Recents")
            .segment("all", "Everything")
            .selected("recents");
        let row = div().flex().items_center().gap(px(12.));
        match self.0 {
            Place::Alone => row.w(px(900.)).child(control),
            Place::BesideSpacer => row
                .w(px(420.))
                .child(div().flex_1().child("Notebooks"))
                .child(control),
            Place::Given => row.w(px(600.)).child(control.w(px(600.))),
        }
    }
}

fn open(cx: &mut TestAppContext, place: Place) -> &mut VisualTestContext {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host(place));
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(gpui::size(px(1000.), px(300.)));
    vcx.run_until_parked();
    vcx
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn widths(vcx: &mut VisualTestContext) -> Vec<Pixels> {
    ["segment-favorites", "segment-recents", "segment-all"]
        .into_iter()
        .map(|id| {
            vcx.debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} drawn"))
                .size
                .width
        })
        .collect()
}

#[gpui::test]
fn beside_a_spacer_the_options_keep_their_labels_whole() {
    let mut cx = app();
    let alone = widths(open(&mut cx, Place::Alone));
    let mut cx = app();
    let beside = widths(open(&mut cx, Place::BesideSpacer));
    assert!(
        alone.iter().all(|w| *w > px(40.)),
        "the labels have width: {alone:?}"
    );
    assert_eq!(
        beside, alone,
        "beside a spacer, as wide as alone (labels whole)"
    );
    // Each as wide as its own label: "Everything" is wider than "Recents".
    assert!(alone[2] > alone[1], "{alone:?}");
}

#[gpui::test]
fn given_a_width_the_options_split_it_equally() {
    let mut cx = app();
    let given = widths(open(&mut cx, Place::Given));
    let f: Vec<f32> = given.iter().map(|w| *w / px(1.)).collect();
    let (lo, hi) = (
        f.iter().copied().fold(f32::MAX, f32::min),
        f.iter().copied().fold(0., f32::max),
    );
    assert!(hi - lo < 1., "equal shares: {given:?}");
    assert!(given[0] > px(150.), "the width split three ways: {given:?}");
}
