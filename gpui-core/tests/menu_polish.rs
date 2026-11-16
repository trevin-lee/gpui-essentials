//! A menu's check in a leading column (every label lined up), its headings' size, and a
//! picker's search field in the host's colours. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{Picker, PopupMenu, PopupMenuItem, Root};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

#[derive(Clone, Copy)]
enum Kind {
    Menu { leading: bool, heading: Option<f32> },
    Picker,
}

struct Panel(Kind);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let body = match self.0 {
            Kind::Menu { leading, heading } => {
                let mut m = PopupMenu::new("menu")
                    .take_focus(false)
                    .check_leading(leading)
                    .heading("Status")
                    .item(PopupMenuItem::new("done", "Done").checked(true))
                    .item(PopupMenuItem::new("todo", "To do").checked(false));
                if let Some(h) = heading {
                    m = m.heading_size(px(h));
                }
                m.into_any_element()
            }
            Kind::Picker => Picker::new("p")
                .placeholder("Find a tag")
                .placeholder_color(RED)
                .item(PopupMenuItem::new("a", "A"))
                .into_any_element(),
        };
        div()
            .p(px(20.))
            .w(px(260.))
            .child(div().debug_selector(|| "box".into()).child(body))
    }
}

fn open(cx: &mut TestAppContext, kind: Kind) -> &mut VisualTestContext {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let v = cx.new(|_| Panel(kind));
        Root::new(v, window, cx)
    });
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

/// The first glyph's left edge in each of the two entries' rows.
fn label_lefts(vcx: &mut VisualTestContext) -> (f32, f32) {
    let done = vcx.debug_bounds("menu-item-done").unwrap();
    let todo = vcx.debug_bounds("menu-item-todo").unwrap();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let left_in = |b: gpui::Bounds<gpui::Pixels>| {
            scene
                .monochrome_sprites
                .iter()
                .map(|g| g.bounds)
                .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
                .filter(|g| {
                    let y = g.origin.y.0 / k;
                    y >= f32::from(b.top()) && y <= f32::from(b.bottom())
                })
                .map(|g| g.origin.x.0 / k)
                .fold(f32::MAX, f32::min)
        };
        (left_in(done), left_in(todo))
    })
}

#[test]
fn a_leading_check_column_lines_every_label_up() {
    let mut cx = app();
    let vcx = open(
        &mut cx,
        Kind::Menu {
            leading: true,
            heading: None,
        },
    );
    let a = vcx
        .debug_bounds("menu-check-done")
        .expect("a column on the checked entry");
    let b = vcx
        .debug_bounds("menu-check-todo")
        .expect("and room on the other");
    assert_eq!((a.left(), a.size.width), (b.left(), b.size.width));
    let (done, todo) = label_lefts(vcx);
    assert!((done - todo).abs() < 0.6, "{done} {todo}");
    assert!(
        done >= f32::from(a.right()),
        "the label after the column: {done} {a:?}"
    );
}

#[test]
fn without_it_theres_no_column() {
    let mut cx = app();
    let vcx = open(
        &mut cx,
        Kind::Menu {
            leading: false,
            heading: None,
        },
    );
    assert!(vcx.debug_bounds("menu-check-done").is_none());
}

#[test]
fn headings_take_the_hosts_size() {
    let h = |size: Option<f32>| {
        let mut cx = app();
        let vcx = open(
            &mut cx,
            Kind::Menu {
                leading: false,
                heading: size,
            },
        );
        vcx.debug_bounds("box").unwrap().size.height
    };
    assert!(
        h(Some(30.)) > h(None) + px(10.),
        "{:?} {:?}",
        h(Some(30.)),
        h(None)
    );
}

#[test]
fn a_pickers_field_takes_the_hosts_colours() {
    let mut cx = app();
    let vcx = open(&mut cx, Kind::Picker);
    let red = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|g| g.color)
            .chain(scene.subpixel_sprites.iter().map(|g| g.color))
            .filter(|c| (c.h - RED.h).abs() < 0.02 && c.s > 0.5)
            .count()
    });
    assert!(red > 3, "the placeholder in red: {red} glyphs");
}
