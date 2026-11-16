//! Pictures through the host's loader, link cards the host fills in, and files dropped on the
//! canvas. The flow fetches and opens nothing itself.

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use common::*;
use gpui::{FileDropEvent, Modifiers, MouseButton, TestAppContext};
use gpui_flow::*;

#[gpui::test]
fn a_picture_takes_its_proportions_and_comes_from_the_hosts_loader(cx: &mut TestAppContext) {
    let asked: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = asked.clone();
    let (flow, _, cx) = open(cx, move |f| {
        f.with_nodes(vec![
            Node::new("pic", xy(0., 0.))
                .image("photos/kyoto.jpg", 1600., 900., "Temples at dawn")
                .label("Kiyomizu-dera"),
            Node::new("bare", xy(400., 0.)).image("", 0., 0., ""),
        ])
        .with_image_loader(move |src, _, _| {
            seen.borrow_mut().push(src.to_string());
            None
        })
    });
    flow.read_with(cx, |f, _| {
        // 1600 × 900 scaled to 320 wide, and the caption's strip under it.
        let r = f.node_rect("pic").unwrap();
        assert_eq!((r.w, r.h), (320., 180. + 26.));
    });
    // Asked for what the node names, and nothing for a node with no picture.
    assert!(asked.borrow().iter().all(|s| s == "photos/kyoto.jpg"));
    assert!(!asked.borrow().is_empty());
}

#[gpui::test]
fn a_picture_keeps_its_proportions_when_resized(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("pic", xy(0., 0.))
                .image("a.png", 400., 200., "")
                .size(200., 100.)
                .selected_now(),
        ])
    });
    let grip = screen(&flow, cx, xy(200., 100.));
    cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        offset(grip, 100., 10.),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        offset(grip, 100., 10.),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    flow.read_with(cx, |f, _| {
        let n = f.node("pic").unwrap();
        assert_eq!((n.width, n.height), (Some(300.), Some(150.)));
    });
}

trait Selected {
    fn selected_now(self) -> Self;
}

impl Selected for Node {
    fn selected_now(mut self) -> Self {
        self.selected = true;
        self
    }
}

#[gpui::test]
fn a_link_card_asks_once_to_be_filled_in_and_opens_nothing_itself(cx: &mut TestAppContext) {
    // Added once the host is listening, as a pasted address would be.
    let (flow, events, cx) = open(cx, |f| f);
    flow.update(cx, |f, cx| {
        f.add_node(
            Node::new("link", xy(0., 0.)).link_card("https://example.com/kyoto"),
            cx,
        );
    });
    let asked = |events: &Events| {
        events
            .borrow()
            .iter()
            .filter(|e| matches!(e, FlowEvent::LinkCardNeeded { node, url } if node == "link" && url == "https://example.com/kyoto"))
            .count()
    };
    draw(cx);
    draw(cx);
    assert_eq!(asked(&events), 1);
    // The host fills it in; it's not asked again.
    flow.update(cx, |f, cx| {
        f.update_node(
            "link",
            |n| {
                let card = n.card.as_mut().unwrap();
                card.title = Some("Kyoto travel guide".into());
                card.site = Some("Example".into());
            },
            cx,
        );
    });
    draw(cx);
    assert_eq!(asked(&events), 1);
    // The open button, at the card's bottom right, asks the host to open it.
    let r = flow.read_with(cx, |f, _| f.node_rect("link").unwrap());
    cx.simulate_click(
        screen(&flow, cx, xy(r.right() - 30., r.bottom() - 16.)),
        Modifiers::none(),
    );
    draw(cx);
    assert!(events.borrow().iter().any(
        |e| matches!(e, FlowEvent::OpenUrl { node, url } if node == "link" && url == "https://example.com/kyoto")
    ));
    // The press was the button's: the card wasn't selected.
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
}

#[gpui::test]
fn files_dropped_on_the_canvas_go_to_the_host(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = screen(&flow, cx, xy(600., 300.));
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: gpui::ExternalPaths(vec![PathBuf::from("/tmp/photo.png")].into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    draw(cx);
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        FlowEvent::FilesDropped { paths, position }
            if paths == &[PathBuf::from("/tmp/photo.png")] && position.distance(xy(600., 300.)) < 1.
    )));
    let _ = flow;
}

#[test]
fn pictures_and_cards_save_and_load() {
    let pic = Node::new("p", xy(0., 0.)).image("a.png", 10., 20., "alt");
    let card = Node::new("c", xy(0., 0.)).link_card("https://x.example");
    for node in [pic, card] {
        let json = serde_json::to_value(&node).unwrap();
        let back: Node = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
    }
}
