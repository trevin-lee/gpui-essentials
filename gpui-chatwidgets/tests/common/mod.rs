//! What the tests share: a host that records what a card hands it, the golden programs, and a
//! window to draw a card in.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_chatwidgets::openui::{ImageSrc, Link, Store};
use gpui_chatwidgets::*;

/// How the test host answers a card's images.
#[derive(Clone, Copy, PartialEq)]
pub enum Images {
    /// Every image refused: the alt text shows.
    Refuse,
    /// Every image at once.
    Ready,
    /// Every image later, when the test delivers it.
    Later,
}

pub struct TestHost {
    pub sent: RefCell<Vec<String>>,
    pub opened: RefCell<Vec<String>>,
    /// Times the card drew (it asks for its theme once a draw).
    pub draws: Cell<usize>,
    pub changes: Cell<usize>,
    pub images: Images,
    pub asked: RefCell<Vec<String>>,
    pub pending: RefCell<Vec<ImageReply>>,
    /// The debug selectors of what scrolls sideways.
    pub scrollers: RefCell<BTreeSet<String>>,
    pub theme: CardTheme,
}

impl TestHost {
    pub fn new(images: Images) -> Rc<TestHost> {
        Rc::new(TestHost {
            sent: RefCell::default(),
            opened: RefCell::default(),
            draws: Cell::new(0),
            changes: Cell::new(0),
            images,
            asked: RefCell::default(),
            pending: RefCell::default(),
            scrollers: RefCell::default(),
            theme: CardTheme {
                rem_base: 16.,
                ..CardTheme::dark()
            },
        })
    }
}

/// A picture, as a host would hand one over.
pub fn picture() -> Arc<Image> {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#4a8"/></svg>"##;
    Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()))
}

impl CardHost for TestHost {
    fn theme(&self, _: &App) -> CardTheme {
        self.draws.set(self.draws.get() + 1);
        self.theme.clone()
    }

    fn send(&self, text: &str, _: &mut App) {
        self.sent.borrow_mut().push(text.to_string());
    }

    fn open_link(&self, link: &Link, _: &mut App) {
        self.opened.borrow_mut().push(link.as_str().to_string());
    }

    fn image(&self, src: &ImageSrc, reply: ImageReply, _: &mut App) -> ImageLoad {
        self.asked.borrow_mut().push(src.as_str().to_string());
        match self.images {
            Images::Refuse => ImageLoad::Refused,
            Images::Ready => ImageLoad::Ready(picture()),
            Images::Later => {
                self.pending.borrow_mut().push(reply);
                ImageLoad::Pending
            }
        }
    }

    fn icon(&self, name: &str, _: &App) -> Option<SharedString> {
        lucide_icon(name)
    }

    fn scroller(&self, selector: &str) {
        self.scrollers.borrow_mut().insert(selector.to_string());
    }

    fn changed(&self, _: &Store, _: &mut App) {
        self.changes.set(self.changes.get() + 1);
    }
}

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/openui")
}

pub fn sample(name: &str) -> String {
    std::fs::read_to_string(dir().join(format!("{name}.oui"))).unwrap()
}

/// Every golden program, by name.
pub fn goldens() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "oui"))
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

/// A window holding one card at a width.
pub struct Frame {
    pub card: Entity<CardView>,
    pub width: Option<Pixels>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .font_family("Inter")
            .child(
                div()
                    .flex_none()
                    .when_some(self.width, |d, w| d.w(w))
                    .child(self.card.clone()),
            )
    }
}

/// The fonts the cards draw with loaded, and gpui-core's theme at the cards' 16 pixel rem.
/// gpui-core sets itself up (its keys) when the window's Root or the first field is made, as in
/// a host that never calls its init.
pub fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_core::Theme::global_mut(cx).font_size = px(16.);
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(
                include_bytes!("../../examples/fonts/Inter-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../examples/fonts/Inter-Medium.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../../examples/fonts/Inter-SemiBold.ttf").as_slice(),
            ),
        ]);
    });
}

/// Opens a card from `source` in a test window, `width` wide (or the window's width).
pub fn open<'a>(
    cx: &'a mut TestAppContext,
    source: &str,
    host: &Rc<TestHost>,
    width: Option<f32>,
) -> (Entity<CardView>, &'a mut VisualTestContext) {
    setup(cx);
    let h: Rc<dyn CardHost> = host.clone();
    let source = source.to_string();
    let made: Rc<RefCell<Option<Entity<CardView>>>> = Rc::default();
    let m = made.clone();
    // In gpui-core's Root, as a host's window is: it moves the focus with Tab.
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let card = cx.new(|cx| CardView::new(&source, "t", h, window, cx));
        *m.borrow_mut() = Some(card.clone());
        let frame = cx.new(|_| Frame {
            card,
            width: width.map(px),
        });
        gpui_core::Root::new(frame, window, cx)
    });
    let card = made.borrow().clone().expect("the card");
    // Tall enough for every card to be drawn whole.
    cx.simulate_resize(size(px(1200.), px(4000.)));
    draw(cx);
    (card, cx)
}

/// Draws the window and lets everything it started finish.
pub fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

/// The middle of what a card marked with `what` (its debug selector, without the prefix).
pub fn spot(cx: &mut VisualTestContext, what: &str) -> Point<Pixels> {
    let sel: &'static str = Box::leak(format!("t:{what}").into_boxed_str());
    cx.debug_bounds(sel)
        .unwrap_or_else(|| panic!("{what} isn't drawn"))
        .center()
}

pub fn drawn(cx: &mut VisualTestContext, what: &str) -> bool {
    let sel: &'static str = Box::leak(format!("t:{what}").into_boxed_str());
    cx.debug_bounds(sel).is_some()
}

/// Clicks what's marked `what`, then draws.
pub fn click(cx: &mut VisualTestContext, what: &str) {
    let at = spot(cx, what);
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
}

/// Every text the window drew, joined (from the card's store and its model: what a person
/// reads, as far as the test can tell without pixels).
pub fn texts(card: &Entity<CardView>, cx: &mut VisualTestContext) -> String {
    card.read_with(cx, |c, _| format!("{:?}", c.card()))
}
