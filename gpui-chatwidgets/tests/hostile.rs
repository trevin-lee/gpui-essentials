//! What an agent (or whoever got into its context) could send: random and cut-off programs,
//! programs pasted into each other, huge tables and charts. The card is drawn and pressed at
//! random; nothing panics, and past the limits a note says what was left out.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use common::*;
use gpui_kit::*;
// The built-in #[test], not GPUI's, which gpui_kit::* brings in.
use std::prelude::v1::test;

/// A small, fixed-seed generator: the same programs every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { self.next() % n }
    }
}

const TOKENS: &[&str] = &[
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    "=",
    "\"",
    "'",
    "\\",
    "$x",
    "@Each(",
    "@Set($x, ",
    "@ToAssistant(",
    "Action([",
    "null",
    "true",
    "-1e309",
    "NaN",
    "Stack(",
    "Card(",
    "Table([Col(\"a\", ",
    "Form(\"f\", ",
    "Modal(\"m\", true, [",
    "BarChart([\"a\"], [Series(\"s\", [",
    "PieChart([\"a\"], [",
    "Tabs([TabItem(\"a\", \"A\", [",
    "Carousel([[",
    "TextContent(\"",
    "Button(\"b\"",
    "Input(\"i\"",
    "Slider(\"s\", \"continuous\", 5, 5, 0, [",
    "\n",
    "root = ",
    "\u{0}",
    "😀",
    "</script>",
];

fn mutate(rng: &mut Lcg, programs: &[(String, String)]) -> String {
    let base = &programs[rng.below(programs.len())].1;
    let cut = |s: &str, at: usize| -> String {
        s.char_indices()
            .take_while(|(i, _)| *i < at)
            .map(|(_, c)| c)
            .collect()
    };
    match rng.below(4) {
        // Cut off anywhere, as a stream that stopped.
        0 => cut(base, rng.below(base.len() + 1)),
        // Another program pasted into the middle.
        1 => {
            let other = &programs[rng.below(programs.len())].1;
            let at = rng.below(base.len() + 1);
            let head = cut(base, at);
            format!(
                "{head}{}{}",
                cut(other, rng.below(other.len() + 1)),
                &base[head.len()..]
            )
        }
        // Tokens sprinkled in.
        2 => {
            let mut s = base.clone();
            for _ in 0..1 + rng.below(12) {
                let at = s
                    .char_indices()
                    .map(|(i, _)| i)
                    .nth(rng.below(s.chars().count() + 1))
                    .unwrap_or(s.len());
                s.insert_str(at, TOKENS[rng.below(TOKENS.len())]);
            }
            s
        }
        // Nothing but tokens, under a root.
        _ => {
            let mut s = String::from("root = Stack([");
            for _ in 0..rng.below(80) {
                s.push_str(TOKENS[rng.below(TOKENS.len())]);
            }
            s
        }
    }
}

#[gpui_kit::test]
fn random_and_cut_programs_draw_and_press_without_panicking(cx: &mut TestAppContext) {
    let programs = goldens();
    let mut rng = Lcg(0x5eed);
    for n in 0..300 {
        let source = mutate(&mut rng, &programs);
        let host = TestHost::new(if n % 2 == 0 {
            Images::Ready
        } else {
            Images::Refuse
        });
        let (card, vcx) = open(cx, &source, &host, Some([280., 400., 736.][n % 3]));
        let Some(bounds) = vcx.debug_bounds("t:card") else {
            panic!("program {n} drew no card:\n{source}")
        };
        // Pressed at random, as a person clicking around would.
        for _ in 0..4 {
            let at = point(
                bounds.origin.x + bounds.size.width * (rng.below(100) as f32 / 100.),
                bounds.origin.y + bounds.size.height * (rng.below(100) as f32 / 100.),
            );
            vcx.simulate_click(at, Modifiers::none());
            draw(vcx);
        }
        vcx.update(|_, cx| card.update(cx, |c, cx| c.expand_all(cx)));
        draw(vcx);
    }
}

#[gpui_kit::test]
fn past_the_limits_a_note_says_what_was_left_out(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let rows: Vec<String> = (0..450).map(|i| format!("\"row {i}\"")).collect();
    let points: Vec<String> = (0..600).map(|i| i.to_string()).collect();
    let labels: Vec<String> = (0..600).map(|i| format!("\"{i}\"")).collect();
    let src = format!(
        "root = Stack([Table([Col(\"Name\", [{}])]), LineChart([{}], [Series(\"s\", [{}])]), CodeBlock(\"txt\", \"{}\")])",
        rows.join(", "),
        labels.join(", "),
        points.join(", "),
        "x".repeat(30_000)
    );
    let (card, cx) = open(cx, &src, &host, Some(400.));
    assert!(card.read_with(cx, |c, _| c.error().is_none()));
    let limits = gpui_chatwidgets::Limits::default();
    assert!(limits.table_rows < 450 && limits.chart_points < 600);
    assert!(drawn(cx, "note"), "the chart's note");
    // The table draws its first rows and counts the rest; nothing past the limit is drawn.
    let card_height = cx.debug_bounds("t:card").unwrap().size.height;
    assert!(card_height < px(20_000.), "{card_height:?}");
}

#[gpui_kit::test]
fn deep_and_huge_cards_fail_cleanly(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let deep = format!(
        "root = {}TextContent(\"x\"){}",
        "Stack([".repeat(40),
        "])".repeat(40)
    );
    let (card, vcx) = open(cx, &deep, &host, Some(400.));
    assert!(card.read_with(vcx, |c, _| c.error().is_some()), "too deep");
    assert!(drawn(vcx, "note"));
    let many = format!(
        "root = Stack([{}])",
        (0..3000)
            .map(|i| format!("TextContent(\"{i}\")"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let (card, vcx) = open(cx, &many, &host, Some(400.));
    // Over MAX_CHILDREN a list is cut short; the card still draws.
    assert!(card.read_with(vcx, |c, _| c.card().is_some()));
}
