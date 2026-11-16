//! Cards driven as a person drives them: a window, the mouse and the keyboard. Every press goes
//! through the card's store, what's left for the host comes back to it, and nothing else
//! happens: no link opened, no message sent, no frame drawn unless something changed.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::time::Duration;

use common::*;
use gpui::*;
use serde_json::json;
// The built-in #[test] (which GPUI's test attribute expands to), not GPUI's, which
// gpui::* brings in.
use std::prelude::v1::test;

#[gpui::test]
fn a_button_hands_its_words_to_the_host(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("buttons"), &host, Some(400.));
    click(cx, "button:Tell me more");
    // Outside a form a press carries the card's whole state, as the web's does.
    assert_eq!(
        host.sent.borrow().as_slice(),
        [
            "Tell me more\n\n```json\n{\n  \"form\": {\n    \"$a\": 1,\n    \"$b\": \"x\"\n  }\n}\n```"
        ]
    );
    click(cx, "button:Ask");
    let ask = host.sent.borrow()[1].clone();
    assert!(
        ask.starts_with(
            "Explain the second chart\n\n```json\n{\n  \"context\": {\n    \"chart\": 2"
        ),
        "{ask}"
    );
    click(cx, "button:Open docs");
    assert_eq!(
        host.opened.borrow().as_slice(),
        ["https://example.com/docs?x=1#top"]
    );
    assert_eq!(host.sent.borrow().len(), 2, "a link isn't a message");
}

#[gpui::test]
fn only_web_links_reach_the_host(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("hostile-links"), &host, Some(400.));
    for b in ["JS", "Data", "File", "User", "Relative", "Legacy js"] {
        click(cx, &format!("button:{b}"));
    }
    assert!(
        host.opened.borrow().is_empty(),
        "{:?}",
        host.opened.borrow()
    );
    click(cx, "button:Upper");
    click(cx, "button:Mixed");
    assert_eq!(
        host.opened.borrow().as_slice(),
        ["https://example.com/Path"]
    );
    assert_eq!(host.sent.borrow().as_slice(), ["still sent"]);
    click(cx, "item:Bad link");
    assert_eq!(host.opened.borrow().len(), 1);
}

#[gpui::test]
fn a_markdown_link_goes_to_the_host_and_markdown_copies(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let program = r#"root = Stack([TextContent("[Docs](https://example.com/docs) and more"), TextContent("[Bad](javascript:alert(1)) and more"), MarkDownRenderer("Copy **these** words")])"#;
    let (_, cx) = open(cx, program, &host, Some(400.));
    // The first word of each is its link.
    let first_word = |cx: &mut VisualTestContext, what: &str| {
        let b = cx
            .debug_bounds(Box::leak(format!("t:md:{what}").into_boxed_str()))
            .unwrap_or_else(|| panic!("{what} isn't drawn"));
        b.origin + point(px(4.), px(8.))
    };
    let docs = first_word(cx, "r.c0");
    cx.simulate_click(docs, Modifiers::none());
    draw(cx);
    assert_eq!(
        host.opened.borrow().as_slice(),
        ["https://example.com/docs"]
    );
    let bad = first_word(cx, "r.c1");
    cx.simulate_click(bad, Modifiers::none());
    draw(cx);
    assert_eq!(
        host.opened.borrow().len(),
        1,
        "only web links reach the host"
    );
    // Dragged across, the text is selected, and Cmd-C copies it (as plain text).
    let b = cx.debug_bounds("t:md:r.c2").expect("the Markdown");
    let (from, to) = (
        b.origin + point(px(1.), px(8.)),
        point(b.right() - px(1.), b.origin.y + px(8.)),
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    key(cx, "cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("Copy these words")
    );
    assert!(host.opened.borrow().len() == 1 && host.sent.borrow().is_empty());
}

#[gpui::test]
fn markdown_shows_html_as_text_and_images_as_their_alt_text(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    // An HTML block would swallow the Markdown after it up to a blank line if it were parsed.
    let program = r#"root = Stack([MarkDownRenderer("<div onclick=\"x()\">\n**after** ![cat](https://evil.test/x.png)")])"#;
    let (_, cx) = open(cx, program, &host, Some(600.));
    let b = cx.debug_bounds("t:md:r.c0").expect("the Markdown");
    let (from, to) = (
        b.origin + point(px(1.), px(4.)),
        point(b.right() - px(1.), b.bottom() - px(1.)),
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    key(cx, "cmd-c");
    let copied = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .expect("copied text");
    assert!(copied.starts_with("<div onclick=\"x()\">"), "{copied:?}");
    // The Markdown after it is still Markdown, and the image only its words.
    assert!(copied.ends_with("after cat"), "{copied:?}");
    assert!(host.asked.borrow().is_empty() && host.opened.borrow().is_empty());
}

#[gpui::test]
fn a_press_sets_variables_and_the_card_draws_from_them(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, &sample("syntax-each-actions"), &host, Some(400.));
    assert!(texts(&card, cx).contains("Picked: none (0)"));
    click(cx, "button:Okafor");
    click(cx, "button:Lindqvist");
    assert!(
        texts(&card, cx).contains("Picked: Lindqvist (2)"),
        "{}",
        texts(&card, cx)
    );
    assert!(host.sent.borrow()[0].starts_with("Pick Okafor"));
    assert!(host.changes.get() >= 2, "the host hears of each change");
    click(cx, "button:Start over");
    assert!(texts(&card, cx).contains("Picked: none (0)"));
}

#[gpui::test]
fn a_form_checks_its_rules_then_sends_its_values(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, &sample("form-contact"), &host, Some(400.));
    click(cx, "button:Submit");
    assert!(
        host.sent.borrow().is_empty(),
        "nothing goes while a rule is broken"
    );
    for f in ["name", "email", "country", "message"] {
        assert!(drawn(cx, &format!("error:{f}")), "{f}'s message shows");
    }
    // Typed in, as a person types.
    for (field, text) in [
        ("r.c1/f0", "Ann Lee"),
        ("r.c1/f1", "ann@example.com"),
        ("r.c1/f3", "A message long enough."),
    ] {
        let at = cx
            .debug_bounds(Box::leak(format!("field:t:{field}.field").into_boxed_str()))
            .expect("the field")
            .center();
        cx.simulate_click(at, Modifiers::none());
        draw(cx);
        cx.simulate_input(text);
        draw(cx);
    }
    click(cx, "select:r.c1/f2");
    click(cx, "pick:United Kingdom");
    assert_eq!(
        card.read_with(cx, |c, _| c.store().field(Some("contact"), "country")),
        Some(json!("uk"))
    );
    click(cx, "button:Submit");
    let sent = host.sent.borrow().clone();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        sent[0].starts_with("Submit\n\n```json\n{\n  \"form contact\": {"),
        "{}",
        sent[0]
    );
    for want in [
        "Ann Lee",
        "ann@example.com",
        "\"uk\"",
        "A message long enough.",
    ] {
        assert!(sent[0].contains(want), "{want} in {}", sent[0]);
    }
    assert!(!drawn(cx, "error:name"));
    // Cancel isn't primary, so it doesn't check, and it carries the form too.
    click(cx, "button:Cancel");
    assert!(host.sent.borrow()[1].starts_with("Cancel"));
}

#[gpui::test]
fn choices_write_the_store(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, &sample("form-inputs"), &host, Some(736.));
    let field =
        |card: &Entity<gpui_chatwidgets::CardView>, cx: &mut VisualTestContext, name: &str| {
            card.read_with(cx, |c, _| c.store().field(Some("prefs"), name))
        };
    // Chips and option cards start with their defaults in the form, as on the web.
    assert_eq!(field(&card, cx, "tools"), Some(json!(["figma"])));
    assert_eq!(field(&card, cx, "style"), Some(json!("collab")));
    click(cx, "chip:Figma");
    assert_eq!(field(&card, cx, "tools"), Some(json!([])));
    click(cx, "chip:Slack");
    assert_eq!(
        field(&card, cx, "tools"),
        Some(json!([])),
        "a disabled chip can't be chosen"
    );
    click(cx, "choice:Go");
    assert_eq!(
        field(&card, cx, "topics"),
        Some(json!({"rust": true, "go": true}))
    );
    click(cx, "choice:Pro");
    assert_eq!(field(&card, cx, "plan"), Some(json!("pro")));
    click(cx, "option:Deep Focus");
    assert_eq!(field(&card, cx, "style"), Some(json!("focused")));
    // A select bound to a $variable writes the variable.
    click(cx, "select:r.c0/f4");
    click(cx, "pick:Small");
    assert_eq!(
        card.read_with(cx, |c, _| c.store().var("$size")),
        Some(json!("s"))
    );
    click(cx, "choice:Email");
    assert_eq!(
        card.read_with(cx, |c, _| c.store().field(None, "alerts")),
        Some(json!({"email": false, "sms": false}))
    );
}

#[gpui::test]
fn text_fields_and_the_slider_write_the_store_from_the_keyboard_and_the_pointer(
    cx: &mut TestAppContext,
) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, &sample("form-inputs"), &host, Some(736.));
    let field =
        |card: &Entity<gpui_chatwidgets::CardView>, cx: &mut VisualTestContext, name: &str| {
            card.read_with(cx, |c, _| c.store().field(Some("prefs"), name))
        };
    // A click puts the focus in a field, and Tab moves it through the fields in order.
    let at = cx
        .debug_bounds("field:t:r.c0/f0.field")
        .expect("the website field")
        .center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    cx.simulate_input("https://a.example");
    key(cx, "tab");
    cx.simulate_input("42");
    key(cx, "tab");
    cx.simulate_input("s3cret");
    key(cx, "tab");
    // Enter in a text area is a new line, not a press.
    cx.simulate_input("Hi");
    key(cx, "enter");
    cx.simulate_input("there");
    draw(cx);
    assert_eq!(field(&card, cx, "site"), Some(json!("https://a.example")));
    assert_eq!(field(&card, cx, "age"), Some(json!("42")));
    assert_eq!(field(&card, cx, "code"), Some(json!("s3cret")));
    assert_eq!(field(&card, cx, "notes"), Some(json!("Hi\nthere")));
    assert!(host.sent.borrow().is_empty(), "nothing was sent");

    // The slider: a press on its track moves it there, an arrow key steps it, and a drag
    // follows the pointer to where it's let go.
    let track = cx.debug_bounds("t:slider:r.c0/f6").expect("the slider");
    let (left, right) = (
        point(track.left() + px(1.), track.center().y),
        point(track.right() - px(1.), track.center().y),
    );
    assert_eq!(
        field(&card, cx, "budget"),
        None,
        "untouched, it's its default"
    );
    cx.simulate_click(left, Modifiers::none());
    draw(cx);
    assert_eq!(field(&card, cx, "budget"), Some(json!([0])));
    key(cx, "right");
    assert_eq!(field(&card, cx, "budget"), Some(json!([50])));
    cx.simulate_mouse_down(left, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(right, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(right, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(field(&card, cx, "budget"), Some(json!([500])));
}

/// Draws frames as the display would, until nothing more is asked, and says how many there were.
fn run_frames(cx: &mut VisualTestContext) -> usize {
    for frames in 0..200 {
        let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
        if asked == 0 {
            return frames;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
        draw(cx);
    }
    panic!("the frames never stopped");
}

#[gpui::test]
fn tabs_and_sections_switch_with_motion_that_ends(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("tabs-accordion"), &host, Some(400.));
    assert!(
        !drawn(cx, "field:t:r.c0.s1.0.table"),
        "the second tab's table isn't drawn yet"
    );
    click(cx, "tab:Details");
    assert!(
        cx.debug_bounds("field:t:r.c0.s1.0.table").is_some(),
        "the chosen tab's content is"
    );
    assert!(!drawn(cx, "open:What is due?"));
    click(cx, "section:What is due?");
    assert!(drawn(cx, "open:What is due?"));
    // The chevron turns and the content fades in over real frames, then it all stops.
    let frames = run_frames(cx);
    assert!(frames >= 3, "it moved over {frames} frames");
    click(cx, "section:What is due?");
    assert!(!drawn(cx, "open:What is due?"));
    run_frames(cx);
    assert_eq!(run_frames(cx), 0);
}

#[gpui::test]
fn a_carousel_slides_one_way_and_back(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("steps-carousel-separator"), &host, Some(400.));
    assert!(drawn(cx, "r.c2:slide0"));
    click(cx, "r.c2:next");
    assert!(drawn(cx, "r.c2:slide1"));
    assert!(run_frames(cx) >= 3, "the slide moves in");
    click(cx, "r.c2:next");
    click(cx, "r.c2:next");
    assert!(drawn(cx, "r.c2:slide2"), "it stops at the last");
    click(cx, "r.c2:previous");
    assert!(drawn(cx, "r.c2:slide1"));
}

#[gpui::test]
fn a_modal_opens_checks_and_closes_through_its_variable(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, &sample("modal"), &host, Some(736.));
    assert!(!drawn(cx, "close:Edit note"));
    click(cx, "button:Edit");
    assert!(drawn(cx, "close:Edit note"));
    assert_eq!(
        card.read_with(cx, |c, _| c.store().var("$showEdit")),
        Some(json!(true))
    );
    click(cx, "close:Edit note");
    assert!(!drawn(cx, "close:Edit note"));
    assert_eq!(
        card.read_with(cx, |c, _| c.store().var("$showEdit")),
        Some(json!(false))
    );
    // Escape closes it too.
    click(cx, "button:Edit");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!drawn(cx, "close:Edit note"));
    // Save checks the title first, then sends, closes and shows the toast for three seconds.
    click(cx, "button:Edit");
    click(cx, "button:Save");
    assert!(host.sent.borrow().is_empty());
    assert!(drawn(cx, "error:title"));
    let at = cx
        .debug_bounds("field:t:r.c1.c0/f0.field")
        .expect("the title field")
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.simulate_input("Groceries");
    draw(cx);
    click(cx, "button:Save");
    assert_eq!(host.sent.borrow().len(), 1);
    assert!(host.sent.borrow()[0].contains("Groceries"));
    assert_eq!(
        card.read_with(cx, |c, _| c.store().var("$saved")),
        Some(json!(true))
    );
    cx.executor().advance_clock(Duration::from_secs(4));
    draw(cx);
    assert_eq!(
        card.read_with(cx, |c, _| c.store().var("$saved")),
        Some(json!(false)),
        "the toast hid itself"
    );
}

#[gpui::test]
fn an_editable_table_saves_its_edits_as_the_web_does(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("editable-table"), &host, Some(736.));
    click(cx, "cell:0:0");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Sam Park");
    cx.simulate_keystrokes("enter");
    draw(cx);
    click(cx, "select:r.c1.e1.1");
    click(cx, "pick:Engineering");
    click(cx, "button:Save Changes");
    let sent = host.sent.borrow().clone();
    assert_eq!(sent.len(), 1, "{sent:?}");
    let s = &sent[0];
    assert!(s.starts_with("Save Changes\n\n```json\n{\n  \"form roster\": {\n    \"roster\": {\n      \"roster\": {\n        \"value\": ["), "{s}");
    assert!(
        s.contains("\"Sam Park\"") && s.contains("\"componentType\": \"EditableTable\""),
        "{s}"
    );
    assert!(s.matches("\"eng\"").count() == 2, "both rows' roles: {s}");
}

#[gpui::test]
fn images_come_only_from_the_host(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Later);
    let (card, cx) = open(cx, &sample("images"), &host, Some(400.));
    let asked = host.asked.borrow().clone();
    // The card asks about every source and loads none itself; the host decides.
    assert!(asked.contains(&"/api/files/content?path=school/supply-demand.png".to_string()));
    assert!(asked.contains(&"https://evil.test/track.png".to_string()));
    assert_eq!(
        asked.iter().collect::<std::collections::HashSet<_>>().len(),
        asked.len(),
        "once per source"
    );
    let draws = host.draws.get();
    let reply = host.pending.borrow_mut().remove(0);
    cx.update(|_, cx| reply.deliver(Some(picture()), cx));
    draw(cx);
    assert!(
        host.draws.get() > draws,
        "an image that arrives draws the card again"
    );
    draw(cx);
    assert_eq!(
        host.asked.borrow().len(),
        asked.len(),
        "and isn't asked for again"
    );
    let _ = card;
}

#[gpui::test]
fn nothing_draws_while_nothing_changes(cx: &mut TestAppContext) {
    for name in [
        "form-contact",
        "charts-2d",
        "tabs-accordion",
        "steps-carousel-separator",
        "content",
    ] {
        let host = TestHost::new(Images::Ready);
        let (_, vcx) = open(cx, &sample(name), &host, Some(400.));
        run_frames(vcx);
        let draws = host.draws.get();
        vcx.executor().advance_clock(Duration::from_secs(10));
        vcx.run_until_parked();
        assert_eq!(
            vcx.update(|window, cx| window.simulate_next_frame(cx)),
            0,
            "{name} asks for no frames"
        );
        assert_eq!(host.draws.get(), draws, "{name} didn't draw again");
    }
}

/// A key pressed and let go (a button acts when the key comes up, as on the web).
fn key(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    draw(cx);
}

#[gpui::test]
fn the_keyboard_reaches_every_button(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (_, cx) = open(cx, &sample("buttons"), &host, Some(400.));
    // The first stop in the window's tab order is the first button.
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    key(cx, "enter");
    assert_eq!(host.sent.borrow().len(), 1, "Enter presses it");
    assert!(host.sent.borrow()[0].starts_with("Tell me more"));
    key(cx, "tab");
    key(cx, "space");
    assert_eq!(
        host.sent.borrow().len(),
        2,
        "Tab moves to the next and Space presses it"
    );
    assert!(host.sent.borrow()[1].starts_with("Explain the second chart"));
}

#[gpui::test]
fn a_card_that_cant_be_read_says_so(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let (card, cx) = open(cx, "just some prose", &host, Some(400.));
    assert_eq!(
        card.read_with(cx, |c, _| c.error().cloned()),
        Some(gpui_chatwidgets::openui::CardError::NoRoot)
    );
    assert!(drawn(cx, "note"), "a note takes its place");
}

#[gpui::test]
fn a_card_too_large_says_so(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let big = format!(
        "root = Stack([TextContent(\"{}\")])",
        "x".repeat(gpui_chatwidgets::openui::MAX_SOURCE_BYTES)
    );
    let (card, cx) = open(cx, &big, &host, Some(400.));
    assert_eq!(
        card.read_with(cx, |c, _| c.error().cloned()),
        Some(gpui_chatwidgets::openui::CardError::TooLarge)
    );
    assert!(drawn(cx, "note"));
}

#[gpui::test]
fn date_pickers_show_and_write_the_store_s_dates(cx: &mut TestAppContext) {
    let host = TestHost::new(Images::Refuse);
    let program = r#"root = Stack([Form("trip", Buttons([Button("Move", @Set($d, "2026-05-06"), "secondary")]), [FormControl("Day", DatePicker("day", "single", null, $d)), FormControl("Stay", DatePicker("stay", "range", null, $r))])])
$d = "2026-03-04"
$r = {from: "2026-01-10", to: "2026-01-12"}"#;
    let (card, cx) = open(cx, program, &host, Some(400.));
    let var = |card: &Entity<gpui_chatwidgets::CardView>, cx: &mut VisualTestContext, name| {
        card.read_with(cx, |c, _| c.store().var(name))
    };
    let open_field = |cx: &mut VisualTestContext, sel: &'static str| {
        let at = cx.debug_bounds(sel).expect("the date field").center();
        cx.simulate_click(at, Modifiers::none());
        draw(cx);
    };
    // The month opens on the stored day: one to the right and Enter picks the next.
    open_field(cx, "field:t:r.c0/f0.date");
    key(cx, "right");
    key(cx, "enter");
    assert_eq!(var(&card, cx, "$d"), Some(json!("2026-03-05")));
    // A press that sets the $variable moves the picker too.
    click(cx, "button:Move");
    open_field(cx, "field:t:r.c0/f0.date");
    key(cx, "right");
    key(cx, "enter");
    assert_eq!(var(&card, cx, "$d"), Some(json!("2026-05-07")));

    // A range: Enter starts a new one on the stored start, and the second pick ends it.
    open_field(cx, "field:t:r.c0/f1.date");
    key(cx, "enter");
    assert_eq!(
        var(&card, cx, "$r"),
        Some(json!({"from": "2026-01-10", "to": null}))
    );
    key(cx, "down");
    key(cx, "enter");
    assert_eq!(
        var(&card, cx, "$r"),
        Some(json!({"from": "2026-01-10", "to": "2026-01-17"}))
    );
    // Escape closes the month without picking.
    open_field(cx, "field:t:r.c0/f1.date");
    assert!(
        cx.debug_bounds("day-2026-01-10").is_some(),
        "the month is open"
    );
    key(cx, "escape");
    assert!(
        cx.debug_bounds("day-2026-01-10").is_none(),
        "Escape closed it"
    );
    assert!(host.sent.borrow().is_empty(), "nothing was sent");
}
