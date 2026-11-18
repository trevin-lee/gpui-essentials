//! A kanban board: three columns of cards that move within and between columns by mouse,
//! trackpad, touch and keyboard (Tab to a column, the arrows to a card, Space to pick it up,
//! the arrows to move it, across to the next column too, Space to drop). Each column scrolls,
//! and scrolls by itself while a card is dragged near its ends.
//!
//! ```sh
//! cargo run -p gpui-dnd --example kanban
//! cargo run -p gpui-dnd --features shots --example kanban -- --shot out.png --scene drag
//! ```

#[path = "shared/mod.rs"]
mod shared;

use gpui::*;
use gpui_core::ActiveTheme as _;
use gpui_dnd::*;
use shared::{Step, hold};

const PAD: f32 = 24.;
const HEADER: f32 = 56.;
const COLUMN: f32 = 240.;
const COLUMN_GAP: f32 = 16.;
const TITLE: f32 = 36.;
const CARD: f32 = 64.;
/// Half a rem, the lists' gap, at gpui-core's 15 pixel rem.
const GAP: f32 = 7.5;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Card {
    id: u32,
    title: &'static str,
    tag: &'static str,
}

struct Board {
    columns: Vec<(&'static str, Vec<Card>, ScrollHandle)>,
}

impl Board {
    fn new() -> Self {
        let card = |id, title, tag| Card { id, title, tag };
        Self {
            columns: vec![
                (
                    "To do",
                    vec![
                        card(1, "Sketch the onboarding", "Design"),
                        card(2, "Draft the brief", "Writing"),
                        card(3, "Price the plans", "Research"),
                        card(4, "Book the venue", "Events"),
                    ],
                    ScrollHandle::new(),
                ),
                (
                    "Doing",
                    vec![
                        card(5, "Review the contract", "Legal"),
                        card(6, "Fix the sync bug", "Engineering"),
                    ],
                    ScrollHandle::new(),
                ),
                (
                    "Done",
                    vec![card(7, "Ship the beta", "Engineering")],
                    ScrollHandle::new(),
                ),
            ],
        }
    }

    fn apply(&mut self, m: &SortableMove<Card>) {
        let column = |id: &ElementId| {
            self.columns
                .iter()
                .position(|(name, _, _)| ElementId::from(*name) == *id)
        };
        let (Some(from), Some(to)) = (column(&m.from.0), column(&m.to.0)) else {
            return;
        };
        let card = self.columns[from].1.remove(m.from.1);
        self.columns[to].1.insert(m.to.1, card);
    }
}

fn card_view(card: &Card, item: &SortableItem, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .h(px(CARD))
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_col()
        .justify_between()
        .rounded(theme.radius_lg())
        .border_1()
        .border_color(if item.preview {
            theme.ring
        } else {
            theme.border
        })
        .bg(theme.card)
        .text_color(theme.card_foreground)
        .text_sm()
        .child(card.title)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(card.tag),
        )
        .into_any_element()
}

impl Render for Board {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity().downgrade();
        let columns = self.columns.iter().map(|(name, cards, scroll)| {
            let this = this.clone();
            div()
                .w(px(COLUMN))
                .flex_none()
                .flex()
                .flex_col()
                .rounded(theme.radius_xl())
                .bg(theme.muted)
                .p(px(8.))
                .child(
                    div()
                        .h(px(TITLE - 8.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_sm()
                        .child(*name)
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(cards.len().to_string()),
                        ),
                )
                .child(
                    Sortable::new(*name, cards.clone())
                        .group("board")
                        .label(*name)
                        .item_label(|c: &Card| c.title.into())
                        .item(|c, item, _, cx| card_view(c, item, cx))
                        .empty(|_, cx| {
                            div()
                                .h(px(CARD))
                                .rounded(cx.theme().radius_lg())
                                .border_1()
                                .border_dashed()
                                .border_color(cx.theme().border)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Drop a card here")
                                .into_any_element()
                        })
                        .on_move(move |m, _, cx| {
                            this.update(cx, |board, cx| {
                                board.apply(m);
                                cx.notify();
                            })
                            .ok();
                        })
                        .scroll(scroll)
                        .max_h(px(380.))
                        .min_h(px(CARD)),
                )
        });
        DndProvider::new(
            div()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .p(px(PAD))
                .flex()
                .flex_col()
                .child(div().h(px(HEADER - PAD)).text_lg().child("Launch board"))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(COLUMN_GAP))
                        .children(columns),
                ),
        )
    }
}

/// The middle of card `row` of column `column`, as the board lays them out.
fn card_at(column: usize, row: usize) -> Point<Pixels> {
    point(
        px(PAD + column as f32 * (COLUMN + COLUMN_GAP) + COLUMN / 2.),
        px(PAD + (HEADER - PAD) + 8. + (TITLE - 8.) + row as f32 * (CARD + GAP) + CARD / 2.),
    )
}

fn script(scene: &str) -> Vec<Step> {
    match scene {
        // "Draft the brief" held over Doing, between its two cards.
        "drag" => hold(card_at(0, 1), card_at(1, 1) - point(px(-12.), px(10.))),
        // The same by keyboard: Tab to To do, down to the second card, pick it up, move it
        // across to Doing.
        "keyboard" => vec![
            Step::Key("tab"),
            Step::Key("down"),
            Step::Key("space"),
            Step::Key("right"),
        ],
        _ => Vec::new(),
    }
}

fn main() {
    shared::run(
        "Kanban",
        size(px(800.), px(520.)),
        |_, cx| cx.new(|_| Board::new()),
        script,
    );
}
