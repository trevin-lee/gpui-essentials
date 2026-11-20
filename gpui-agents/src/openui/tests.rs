//! The parser against lang-core itself: each tests/openui/<name>.oui read here and by the web's
//! parser (tests/openui/golden.mjs wrote <name>.json from @openuidev/lang-core 1.1.0), and the
//! two compared: what parsed, every issue, unresolved and orphaned names, state defaults, and the
//! root as the web first draws it. Then the typed model, the caps (with inputs made here, too big
//! to commit), hostile input and the action contract.

use super::eval::Eval;
use super::value::{V, to_serde};
use super::*;
use serde_json::{Value as J, json};
use std::path::PathBuf;

const ORIGIN: &str = "https://app.example.com";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/openui")
}

/// A value as golden.mjs writes one: elements as { type, typeName, props }, expressions as
/// their AST, numbers that aren't finite as null.
fn shown(v: &V) -> J {
    match v {
        V::El(e) => {
            let props: serde_json::Map<String, J> = e
                .props
                .iter()
                .filter(|(_, x)| !matches!(x, V::Undef))
                .map(|(k, x)| (k.to_string(), shown(x)))
                .collect();
            json!({ "type": "element", "typeName": &*e.name, "props": props })
        }
        V::Arr(a) => J::Array(a.iter().map(shown).collect()),
        V::Obj(o) => J::Object(
            o.iter()
                .filter(|(_, x)| !matches!(x, V::Undef))
                .map(|(k, x)| (k.to_string(), shown(x)))
                .collect(),
        ),
        V::Plan(steps) => json!({ "steps": steps.iter().map(shown).collect::<Vec<_>>() }),
        V::Ast(a) => shown(&a.to_value()),
        other => to_serde(other),
    }
}

/// Every number as a double, so 1 and 1.0 compare equal (serde_json keeps them apart).
fn norm(j: J) -> J {
    match j {
        J::Number(n) => {
            serde_json::Number::from_f64(n.as_f64().unwrap_or(0.0)).map_or(J::Null, J::Number)
        }
        J::Array(a) => J::Array(a.into_iter().map(norm).collect()),
        J::Object(m) => J::Object(m.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        other => other,
    }
}

fn golden(src: &str) -> J {
    let mut budget = Budget::new();
    let p = parse::parse(src, &mut budget).expect("parses within the caps");
    let store = Store::new(&p.state);
    let mut budget = Budget::new();
    let state = |name: &str| store.state(name);
    let root = Eval::new(&mut budget, &state).root(p.root.as_ref(), p.root_expression.as_ref());
    let errors: Vec<J> = p
        .issues
        .iter()
        .map(|i| {
            let mut e = json!({ "code": i.code, "component": i.component, "path": i.path, "statementId": i.statement });
            if i.warning {
                e["severity"] = json!("warning");
            }
            e
        })
        .collect();
    json!({
        "parsed": p.root.is_some(),
        "root": root.as_ref().map_or(J::Null, shown),
        "errors": errors,
        "unresolved": p.unresolved,
        "orphaned": p.orphaned,
        "incomplete": p.incomplete,
        "statementCount": p.statements,
        "state": shown(&V::obj(p.state.clone())),
    })
}

/// The first place two JSON values differ, as a path.
fn diff(a: &J, b: &J, path: &str) -> Option<String> {
    match (a, b) {
        (J::Object(x), J::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = diff(p, q, &format!("{path}/{k}")) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{path}/{k}: {p:?} vs {q:?}")),
                }
            }
            None
        }
        (J::Array(x), J::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!(
                    "{path}: {} items vs {}: {a} vs {b}",
                    x.len(),
                    y.len()
                ));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (p, q))| diff(p, q, &format!("{path}/{i}")))
        }
        _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
    }
}

#[test]
fn reads_every_sample_as_lang_core_does() {
    let mut names: Vec<_> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "oui"))
        .collect();
    names.sort();
    assert!(names.len() >= 40, "the samples are missing");
    let mut failures = Vec::new();
    for path in &names {
        let src = std::fs::read_to_string(path).unwrap();
        let expected: J = serde_json::from_str(
            &std::fs::read_to_string(path.with_extension("json"))
                .expect("golden.mjs has written its JSON"),
        )
        .unwrap();
        let got = golden(&src);
        if let Some(d) = diff(&norm(got), &norm(expected), "") {
            failures.push(format!(
                "{}: {d}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ from lang-core:\n{}",
        failures.len(),
        names.len(),
        failures.join("\n")
    );
}

fn sample(name: &str) -> String {
    std::fs::read_to_string(dir().join(format!("{name}.oui"))).unwrap()
}

fn card(src: &str) -> (Program, Store, Card) {
    let p = parse(src).expect("a card");
    let store = p.store();
    let c = p.render(&store).expect("renders");
    (p, store, c)
}

fn children(c: &Component) -> &[Component] {
    match c {
        Component::Stack(f) | Component::Card(f) => &f.children,
        other => panic!("not a container: {other:?}"),
    }
}

/// Every component in the tree, depth first.
fn walk<'a>(c: &'a Component, out: &mut Vec<&'a Component>) {
    out.push(c);
    let mut kids: Vec<&Component> = Vec::new();
    match c {
        Component::Stack(f) | Component::Card(f) => kids.extend(&f.children),
        Component::Tabs(s) | Component::Accordion(s) => {
            kids.extend(s.iter().flat_map(|s| &s.content))
        }
        Component::Carousel { slides, .. } => kids.extend(slides.iter().flatten()),
        Component::Modal(m) => kids.extend(&m.children),
        Component::Table(t) => kids.extend(t.rows.iter().flatten().flat_map(|c| {
            if let Cell::Components(v) = c {
                v.as_slice()
            } else {
                &[]
            }
        })),
        Component::Cards(b) => kids.extend(b.items.iter().flat_map(|i| {
            i.header
                .iter()
                .map(|h| &**h)
                .chain(&i.body)
                .chain(&i.footer)
        })),
        _ => {}
    }
    for k in kids {
        walk(k, out);
    }
}

fn all(c: &Card) -> Vec<&Component> {
    let mut out = Vec::new();
    walk(&c.root, &mut out);
    out
}

#[test]
fn every_component_of_the_library_has_a_shape() {
    let names = [
        "layout-stack-card",
        "tabs-accordion",
        "steps-carousel-separator",
        "modal",
        "content",
        "images",
        "table",
        "editable-table",
        "charts-2d",
        "charts-1d",
        "scatter",
        "form-contact",
        "form-inputs",
        "buttons",
        "data-display",
        "cards-kpi",
        "cards-more",
    ];
    let mut kinds = std::collections::BTreeSet::new();
    for n in names {
        let (_, _, c) = card(&sample(n));
        for comp in all(&c) {
            let k = format!("{comp:?}");
            kinds.insert(k.split(['(', ' ', '{']).next().unwrap().to_string());
            assert!(!matches!(comp, Component::Unknown(_)), "{n}: {comp:?}");
        }
    }
    for k in [
        "Stack",
        "Card",
        "Tabs",
        "Accordion",
        "Steps",
        "Carousel",
        "Separator",
        "Modal",
        "Header",
        "Text",
        "Markdown",
        "Callout",
        "Image",
        "Gallery",
        "Code",
        "InlineHeader",
        "Table",
        "EditableTable",
        "Chart",
        "Form",
        "Field",
        "Label",
        "Button",
        "Buttons",
        "IconButton",
        "TagBlock",
        "Tag",
        "Icon",
        "EntityList",
        "List",
        "Cards",
        "Line",
        "IconText",
        "ImageText",
        "Metric",
        "Plain",
    ] {
        assert!(kinds.contains(k), "no sample draws a {k}: {kinds:?}");
    }
}

#[test]
fn a_layout_reads_with_its_defaults() {
    let (_, _, c) = card(&sample("layout-stack-card"));
    let Component::Stack(root) = &c.root else {
        panic!()
    };
    assert_eq!(
        (
            root.direction,
            root.gap,
            root.align,
            root.justify,
            root.wrap
        ),
        (
            Direction::Column,
            Gap::L,
            Some(Align::Stretch),
            Some(Justify::Start),
            false
        )
    );
    let Component::Stack(row) = &root.children[1] else {
        panic!()
    };
    assert_eq!(
        (row.direction, row.justify, row.wrap),
        (Direction::Row, Some(Justify::Between), true)
    );
    let Component::Card(sunk) = &row.children[1] else {
        panic!()
    };
    assert_eq!(sunk.surface, Surface::Sunk);
    assert_eq!(
        children(&root.children[3]),
        [
            Component::Plain("A bare string".into()),
            Component::Plain("42".into()),
            Component::Plain("true".into())
        ]
    );
}

#[test]
fn a_table_is_row_by_row_with_components_in_cells() {
    let (_, _, c) = card(&sample("table"));
    let Component::Table(t) = &children(&c.root)[1] else {
        panic!()
    };
    assert_eq!(
        t.columns
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        ["Task", "Hours", "Status", "Actions", "Short"]
    );
    assert_eq!(t.columns[1].kind, ColumnKind::Number);
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.rows[1][0], Cell::Text("Lab report".into()));
    assert_eq!(t.rows[1][1], Cell::Text("1.5".into()));
    assert_eq!(t.rows[2][1], Cell::Empty);
    assert_eq!(t.rows[1][4], Cell::Empty);
    let Cell::Components(tag) = &t.rows[1][2] else {
        panic!()
    };
    assert!(
        matches!(&tag[0], Component::Tag(Tag { text, tone: Tone::Danger, .. }) if text == "late")
    );
    let Cell::Components(btn) = &t.rows[0][3] else {
        panic!()
    };
    let Component::Button(b) = &btn[0] else {
        panic!()
    };
    assert_eq!(
        b.action.steps,
        [Step::ToAssistant {
            message: Some("Close task".into()),
            context: Some(Data(V::obj({
                let mut o = value::Obj::new();
                o.set("id", V::Num(1.0));
                o
            })))
        }]
    );
    assert_eq!(
        children(&c.root)[2],
        Component::Text {
            markdown: "No more data".into(),
            size: TextSize::Default
        }
    );
}

#[test]
fn charts_come_shaped_for_drawing() {
    let (_, _, c) = card(&sample("charts-2d"));
    let kids = children(&c.root);
    let Component::Chart(Chart::Bars(bar)) = &kids[0] else {
        panic!()
    };
    assert_eq!(bar.labels, ["Oct", "Nov", "Dec"]);
    assert_eq!(
        bar.series[1],
        Series {
            name: "Product B".into(),
            values: vec![Some(90.0), Some(-10.0), Some(140.5)]
        }
    );
    assert_eq!(
        (bar.stacked, bar.x_label.as_deref(), bar.height),
        (false, Some("Month"), Some(240.0))
    );
    assert_eq!(bar.range(), (-10.0, 180.0));
    let Component::Chart(Chart::Bars(stacked)) = &kids[1] else {
        panic!()
    };
    assert_eq!(stacked.range(), (-10.0, 320.5));
    let Component::Chart(Chart::Bars(line)) = &kids[2] else {
        panic!()
    };
    assert_eq!(
        (line.curve, line.series[0].values.clone()),
        (Curve::Natural, vec![Some(1.0), Some(2.0), None])
    );
    let Component::Chart(Chart::Bars(h)) = &kids[5] else {
        panic!()
    };
    assert_eq!(h.kind, ChartKind::HorizontalBar);

    let (_, _, c) = card(&sample("charts-1d"));
    let kids = children(&c.root);
    let Component::Chart(Chart::Slices(donut)) = &kids[1] else {
        panic!()
    };
    assert_eq!(donut.kind, SlicesKind::Donut);
    assert_eq!(
        donut.slices.iter().map(|s| s.value).collect::<Vec<_>>(),
        [1.0, 1.0, 2.0]
    );
    assert_eq!(donut.shares(), [0.25, 0.25, 0.5]);
    let Component::Chart(Chart::Slices(semi)) = &kids[2] else {
        panic!()
    };
    assert!(semi.semicircle);
    let Component::Chart(Chart::Slices(neg)) = &kids[5] else {
        panic!()
    };
    assert_eq!(
        neg.slices[1].value, 0.0,
        "a negative slice draws as nothing"
    );

    let (_, _, c) = card(&sample("scatter"));
    let Component::Chart(Chart::Scatter(s)) = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(
        s.sets[0].points[1],
        ScatterPoint {
            x: 2.0,
            y: 70.0,
            z: Some(5.0)
        }
    );
    assert_eq!(s.bounds(), Some(((1.0, 4.0), (55.0, 90.0))));
}

#[test]
fn forms_carry_their_fields_rules_and_bindings() {
    let (_, _, c) = card(&sample("form-inputs"));
    let Component::Form(f) = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(f.name, "prefs");
    assert_eq!(
        f.fields.len(),
        11,
        "a control whose input isn't an input is left out"
    );
    let size = &f.fields[4].field;
    assert_eq!(size.value, Some(Bound::Var("$size".into())));
    assert!(
        matches!(&size.input, Input::Select { options, size: Size::Small, .. } if options.len() == 2)
    );
    assert_eq!(
        f.fields[1].field.rules,
        Rules(vec![Rule::Numeric, Rule::Min(13.0), Rule::Max(120.0)])
    );
    assert_eq!(f.fields[1].hint.as_deref(), Some("Your age in years"));
    assert!(
        matches!(&f.fields[6].field.input, Input::Slider { discrete: true, min: 0.0, max: 500.0, step: Some(50.0), default, .. } if default == &[100.0])
    );
    let Input::CheckBoxes(boxes) = &f.fields[7].field.input else {
        panic!()
    };
    assert_eq!((boxes[0].checked, boxes[1].checked), (true, false));
    let tools = &f.fields[9].field;
    assert_eq!(tools.kind(), Some("Chips"));
    assert!(
        matches!(&tools.input, Input::Chips { options, multiple: true, .. } if options.len() == 2 && options[1].disabled),
        "a chip without a value is left out"
    );
    assert_eq!(tools.hydrated(), Some(json!(["figma"])));
    let style = &f.fields[10].field;
    assert_eq!(style.hydrated(), Some(json!("collab")));
    let Input::OptionCards { options, .. } = &style.input else {
        panic!()
    };
    assert_eq!(
        options[1]
            .image
            .as_ref()
            .and_then(|i| i.src.as_ref())
            .map(ImageSrc::as_str),
        Some("/api/files/content?path=team.png")
    );
    assert!(f.buttons.buttons[0].action.checks_form());
}

#[test]
fn rules_word_their_failures_as_the_web_does() {
    let r = |rules: Vec<Rule>, v: J| Rules(rules).check(&v);
    assert_eq!(
        r(vec![Rule::Required], json!("")),
        Some("This field is required".into())
    );
    assert_eq!(
        r(vec![Rule::Required], json!({"a": false, "b": false})),
        Some("At least one option is required".into())
    );
    assert_eq!(
        r(vec![Rule::Email], json!("a@b")),
        Some("Please enter a valid email".into())
    );
    assert_eq!(r(vec![Rule::Email], json!("a@b.co")), None);
    assert_eq!(
        r(vec![Rule::MinLength(2.0)], json!("😀")),
        None,
        "lengths count UTF-16 units"
    );
    assert_eq!(
        r(vec![Rule::Min(13.0)], json!("12 years")),
        Some("Must be at least 13".into())
    );
    assert_eq!(
        r(vec![Rule::Numeric], json!("abc")),
        Some("Must be a number".into())
    );
    assert_eq!(
        r(vec![Rule::Pattern("^[a-z]+$".into())], json!("Abc")),
        Some("Invalid format".into())
    );
    assert_eq!(
        r(vec![Rule::Pattern("(".into())], json!("x")),
        None,
        "a pattern that doesn't compile checks nothing"
    );
    assert_eq!(
        r(vec![Rule::Pattern("(a{1000}){1000}".into())], json!("aaa")),
        None,
        "a pattern too big to build checks nothing"
    );
    assert_eq!(
        r(vec![Rule::Url], json!("not a url")),
        Some("Please enter a valid URL".into())
    );
}

#[test]
fn buttons_send_what_the_web_sends() {
    let (p, mut store, c) = card(&sample("buttons"));
    let Component::Buttons(group) = &children(&c.root)[0] else {
        panic!()
    };
    let press = |b: &Button, store: &mut Store| p.act(&b.action, &b.label, None, store);
    // No action: the label, with the card's whole state as the web's form payload.
    assert_eq!(press(&group.buttons[0], &mut store), [Effect::Send("Tell me more\n\n```json\n{\n  \"form\": {\n    \"$a\": 1,\n    \"$b\": \"x\"\n  }\n}\n```".into())]);
    // A message with context: the context first, as JSON.stringify writes it.
    let Effect::Send(text) = &press(&group.buttons[1], &mut store)[0] else {
        panic!()
    };
    assert!(text.starts_with("Explain the second chart\n\n```json\n{\n  \"context\": {\n    \"chart\": 2,\n    \"tags\": [\n      \"a\",\n      \"b\"\n    ],\n    \"nested\": {\n      \"ok\": true,\n      \"n\": null\n    }\n  },\n  \"form\": {"), "{text}");
    assert_eq!(
        press(&group.buttons[2], &mut store),
        [Effect::Open(
            Link::parse("https://example.com/docs?x=1#top").unwrap()
        )]
    );
    let both = press(&group.buttons[3], &mut store);
    assert!(
        matches!(&both[..], [Effect::Open(_), Effect::Send(t)] if t.starts_with("Summarize this page"))
    );
    // @Reset then @Set: $a back to 1, then 2.
    store.set_var("$a", json!(5));
    assert!(press(&group.buttons[4], &mut store).is_empty());
    assert_eq!(
        (store.var("$a"), store.var("$b")),
        (Some(json!(2)), Some(json!("x")))
    );
    assert_eq!(
        group.buttons.len(),
        5,
        "a number for a label fails the schema, and the button goes, as on the web"
    );
    let Component::Buttons(legacy) = &children(&c.root)[3] else {
        panic!()
    };
    assert_eq!(
        legacy.buttons[0].action.steps,
        [Step::OpenUrl(
            Link::parse("https://example.com/legacy").unwrap()
        )]
    );
    assert!(matches!(
        &legacy.buttons[1].action.steps[..],
        [Step::ToAssistant {
            message: None,
            context: Some(_)
        }]
    ));
    assert!(
        legacy.buttons[2].action.steps.is_empty(),
        "a string isn't an action: the press does nothing"
    );
}

#[test]
fn a_form_press_carries_its_fields() {
    let (p, mut store, c) = card(&sample("form-contact"));
    let Component::Form(f) = &children(&c.root)[1] else {
        panic!()
    };
    store.set_field(Some("contact"), None, "name", json!("Ann"));
    store.set_field(
        Some("contact"),
        f.fields[2].field.kind(),
        "country",
        json!("uk"),
    );
    let submit = &f.buttons.buttons[0];
    assert_eq!(
        p.act(&submit.action, &submit.label, Some("contact"), &mut store),
        [Effect::Send("Submit\n\n```json\n{\n  \"form contact\": {\n    \"contact\": {\n      \"name\": {\n        \"value\": \"Ann\"\n      },\n      \"country\": {\n        \"value\": \"uk\"\n      }\n    }\n  }\n}\n```".into())]
    );
    assert_eq!(store.field(Some("contact"), "name"), Some(json!("Ann")));
    assert_eq!(store.field(Some("contact"), "missing"), None);
}

#[test]
fn modal_and_toast_follow_their_variables() {
    let (p, mut store, c) = card(&sample("modal"));
    let Component::Modal(m) = &children(&c.root)[1] else {
        panic!()
    };
    assert_eq!(m.open, Bound::Var("$showEdit".into()));
    assert_eq!(store.var("$showEdit"), Some(json!(false)));
    let Component::Button(open) = &children(&c.root)[0] else {
        panic!()
    };
    p.act(&open.action, &open.label, None, &mut store);
    assert_eq!(store.var("$showEdit"), Some(json!(true)));
    let Component::Form(f) = &m.children[0] else {
        panic!()
    };
    let save = &f.buttons.buttons[0];
    let sent = p.act(&save.action, &save.label, Some("note"), &mut store);
    assert!(matches!(&sent[..], [Effect::Send(t)] if t.starts_with("Save the note")));
    assert_eq!(
        (store.var("$showEdit"), store.var("$saved")),
        (Some(json!(false)), Some(json!(true)))
    );
    let after = p.render(&store).unwrap();
    let Component::Callout(toast) = &children(&after.root)[2] else {
        panic!()
    };
    assert_eq!(toast.visible, Some(Bound::Var("$saved".into())));
}

#[test]
fn each_writes_the_item_into_its_steps() {
    let (p, mut store, c) = card(&sample("syntax-each-actions"));
    let Component::Buttons(b) = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(
        b.buttons
            .iter()
            .map(|b| b.label.as_str())
            .collect::<Vec<_>>(),
        ["Okafor", "Lindqvist"]
    );
    let sent = p.act(&b.buttons[1].action, &b.buttons[1].label, None, &mut store);
    let Effect::Send(text) = &sent[0] else {
        panic!()
    };
    assert!(
        text.starts_with(
            "Pick Lindqvist\n\n```json\n{\n  \"context\": {\n    \"id\": \"p2\"\n  },"
        ),
        "{text}"
    );
    assert_eq!(
        (store.var("$picked"), store.var("$count")),
        (Some(json!("Lindqvist")), Some(json!(1)))
    );
    let again = p.render(&store).unwrap();
    assert_eq!(
        children(&again.root)[1],
        Component::Text {
            markdown: "Picked: Lindqvist (1)".into(),
            size: TextSize::Default
        }
    );
    let Component::Button(reset) = &children(&c.root)[2] else {
        panic!()
    };
    p.act(&reset.action, &reset.label, None, &mut store);
    assert_eq!(
        (store.var("$picked"), store.var("$count")),
        (Some(json!("none")), Some(json!(0)))
    );
}

#[test]
fn card_blocks_press_with_the_card_s_details() {
    let (p, mut store, c) = card(&sample("cards-kpi"));
    let Component::Cards(kpis) = &children(&c.root)[1] else {
        panic!()
    };
    assert_eq!(kpis.kind, CardBlockKind::Snippet);
    let press = kpis.items[1].press.as_ref().unwrap();
    assert_eq!(press.words, "Active Users");
    let Effect::Send(text) = &p.act(&press.action, &press.words, None, &mut store)[0] else {
        panic!()
    };
    assert!(text.starts_with("Explain this metric\n\n```json\n{\n  \"context\": \"Selected item: {\\\"itemIndex\\\":1,\\\"itemId\\\":\\\"active-users\\\",\\\"itemTitle\\\":\\\"Active Users\\\",\\\"itemSubtitle\\\":\\\"This month\\\",\\\"itemValue\\\":\\\"1,204\\\"}\""), "{text}");
    let Component::Cards(products) = &children(&c.root)[3] else {
        panic!()
    };
    assert!(products.carousel);
    let item = &products.items[1];
    assert_eq!(item.press.as_ref().unwrap().words, "Widget Mini");
    assert!(matches!(
        item.footer.as_slice(),
        [
            Component::Metric(Metric {
                previous: Some(_),
                trend: Some(Trend { up: false, .. }),
                ..
            }),
            Component::Button(_)
        ]
    ));
    let Step::ToAssistant {
        context: Some(ctx), ..
    } = &item.press.as_ref().unwrap().action.steps[0]
    else {
        panic!()
    };
    assert_eq!(
        ctx.json(),
        r#""{\"source\":\"card\"}\nSelected item: {\"itemIndex\":1,\"itemId\":\"widget-mini\",\"itemHeaderTitle\":\"Widget Mini\",\"itemHeaderSubtitle\":\"New arrival\",\"itemBodyCount\":1,\"itemFooterPrice\":\"$14.00\",\"itemFooterButtonLabel\":\"View Details\"}""#
    );

    let (_, _, c) = card(&sample("cards-more"));
    let Component::Cards(ctx) = &children(&c.root)[1] else {
        panic!()
    };
    assert_eq!(
        ctx.items[1].tag.as_ref().map(|t| t.text.as_str()),
        Some("Alert")
    );
    assert_eq!(
        ctx.items[1]
            .background
            .as_ref()
            .and_then(|b| b.same_origin(ORIGIN)),
        None,
        "a background from another site isn't on the host's origin"
    );
    assert!(ctx.items[0].gray);
    assert!(matches!(&ctx.items[0].press, Some(Press { words, .. }) if words == "Weather"));
    let Component::Cards(visual) = &children(&c.root)[2] else {
        panic!()
    };
    assert_eq!(
        visual.items[0].background.as_ref().map(ImageSrc::as_str),
        Some("/api/files/content?path=trips/lisbon.jpg")
    );
    assert_eq!(visual.items[0].press.as_ref().unwrap().words, "Lisbon");
}

/// A host that serves images from its own origin, at one path, as an app might: what its
/// loader would accept, through [`ImageSrc::same_origin`].
fn shared_file(src: &ImageSrc) -> Option<String> {
    let path = src.same_origin(ORIGIN)?;
    let (p, q) = path.split_once('?')?;
    (p == "/api/files/content"
        && q.split('&')
            .any(|kv| kv.starts_with("path=") && kv.len() > 5))
    .then_some(path)
}

#[test]
fn image_sources_are_data_for_the_host_to_judge() {
    let ok = |s: &str| ImageSrc::parse(s).as_ref().and_then(shared_file);
    assert_eq!(
        ok("/api/files/content?path=school/supply-demand.png").as_deref(),
        Some("/api/files/content?path=school/supply-demand.png")
    );
    assert_eq!(
        ok("https://app.example.com/api/files/content?path=a%20b.png").as_deref(),
        Some("/api/files/content?path=a%20b.png")
    );
    assert_eq!(
        ok("  /api/files/content?path=a.png&download=1 ").as_deref(),
        Some("/api/files/content?path=a.png&download=1")
    );
    for bad in [
        "https://evil.test/api/files/content?path=x.png",
        "//evil.test/api/files/content?path=x.png",
        "http://app.example.com/api/files/content?path=x.png",
        "https://someone@app.example.com/api/files/content?path=x.png",
        "/api/files/content/../users?path=x",
        "/api/files/content?path=",
        "/api/files/content",
        "/api/users/abc",
        "data:image/png;base64,iVBORw0KGgo=",
        "blob:https://app.example.com/123",
        "javascript:alert(1)",
        "",
    ] {
        assert_eq!(ok(bad), None, "{bad}");
    }
    assert_eq!(ImageSrc::parse("   "), None);
    assert_eq!(ImageSrc::parse("a\nb"), None);
    assert_eq!(ImageSrc::parse(&"x".repeat(4097)), None);
    assert_eq!(
        ImageSrc::parse("x.png")
            .unwrap()
            .same_origin("javascript:alert(1)"),
        None
    );
    let (_, _, c) = card(&sample("images"));
    let srcs: Vec<Option<String>> = all(&c)
        .into_iter()
        .filter_map(|x| match x {
            Component::Image(i) => Some(i.src.as_ref().and_then(shared_file)),
            _ => None,
        })
        .collect();
    assert_eq!(
        srcs,
        [
            Some("/api/files/content?path=school/supply-demand.png".into()),
            Some("/api/files/content?path=school/a%20b.png".into()),
            None,
            None,
            None,
            None,
            None
        ]
    );
    let Component::Gallery(g) = &children(&c.root)[2] else {
        panic!()
    };
    assert_eq!(
        g.iter()
            .map(|i| i.src.as_ref().and_then(shared_file).is_some())
            .collect::<Vec<_>>(),
        [true, false, false]
    );
    let (_, _, c) = card(&sample("hostile-images"));
    let kept: Vec<String> = all(&c)
        .iter()
        .filter_map(|x| match x {
            Component::Image(Image { src: Some(s), .. }) => shared_file(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        kept,
        [
            "/api/files/content?path=../../etc/passwd",
            "/api/files/content?path=a.png&download=1"
        ],
        "the host's own path check is what stands; the query is the host's to read"
    );
}

#[test]
fn links_open_only_over_http() {
    let (_, _, c) = card(&sample("hostile-links"));
    let steps: Vec<Vec<Step>> = children(&c.root)
        .iter()
        .filter_map(|x| {
            if let Component::Button(b) = x {
                Some(b.action.steps.clone())
            } else {
                None
            }
        })
        .collect();
    for (i, s) in steps.iter().enumerate().take(5) {
        assert!(s.is_empty(), "button {i}: {s:?}");
    }
    assert_eq!(
        steps[5],
        [Step::OpenUrl(
            Link::parse("https://example.com/Path").unwrap()
        )]
    );
    assert!(steps[6].is_empty());
    assert!(
        matches!(&steps[7][..], [Step::ToAssistant { .. }]),
        "the bad link goes, the message stays"
    );
    let Component::List(l) = &children(&c.root)[8] else {
        panic!()
    };
    assert_eq!(l.items[0].action.as_ref().map(|a| a.steps.len()), Some(0));
    assert_eq!(
        Link::parse("https://example.com")
            .map(|l| l.as_str().to_string())
            .as_deref(),
        Some("https://example.com/")
    );
}

#[test]
fn text_is_only_text() {
    let (p, mut store, c) = card(&sample("hostile-html"));
    let kids = children(&c.root);
    assert_eq!(
        kids[0],
        Component::Text {
            markdown: "<script>alert('x')</script><img src=x onerror=alert(1)>".into(),
            size: TextSize::Default
        }
    );
    assert!(matches!(&kids[2], Component::Code { code, .. } if code == "<b>bold</b>"));
    let Component::Button(b) = &kids[4] else {
        panic!()
    };
    assert_eq!(b.label, "<i>Go</i>");
    assert_eq!(
        p.act(&b.action, &b.label, None, &mut store),
        [Effect::Send("<script>steal()</script>".into())]
    );
}

#[test]
fn steps_made_from_data_are_held_to_what_calls_make() {
    let (p, mut store, c) = card(&sample("hostile-steps-from-data"));
    let Component::Button(b) = &children(&c.root)[0] else {
        panic!()
    };
    // A data object can't carry an expression to run, a custom step or a message that isn't
    // text; its link and message steps are as good as written ones.
    assert_eq!(
        b.action.steps,
        [Step::OpenUrl(
            Link::parse("https://example.com/from-data").unwrap()
        )]
    );
    p.act(&b.action, &b.label, None, &mut store);
    assert_eq!(store.var("$x"), Some(json!("safe")));
}

#[test]
fn malformed_programs_fail_without_panicking() {
    for name in [
        "malformed-empty",
        "malformed-only-comments",
        "malformed-no-equals",
        "syntax-root-not-component",
        "syntax-root-ternary",
    ] {
        assert_eq!(
            parse(&sample(name)).err(),
            Some(CardError::NoRoot),
            "{name}"
        );
    }
    for name in [
        "malformed-garbage",
        "malformed-mixed",
        "malformed-bad-escapes",
        "syntax-unclosed",
        "syntax-positional-errors",
    ] {
        let (p, _, _) = card(&sample(name));
        assert!(
            !p.issues().is_empty()
                || name.starts_with("malformed-bad")
                || name == "syntax-unclosed",
            "{name}"
        );
    }
    // Arbitrary bytes, cut at every point of a real card, never panic.
    let src = sample("cards-kpi");
    for end in (0..src.len())
        .filter(|&i| src.is_char_boundary(i))
        .step_by(7)
    {
        if let Ok(p) = parse(&src[..end]) {
            let _ = p.render(&p.store());
        }
    }
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let alphabet = b"()[]{},:=\"'\\$@#/.?!+-*%<>&|0123456789 \nabcXYZ_\t";
    for _ in 0..300 {
        let text: String = (0..200)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                alphabet[(seed % alphabet.len() as u64) as usize] as char
            })
            .collect();
        if let Ok(p) = parse(&format!("root = Stack([{text}])")) {
            let _ = p.render(&p.store());
        }
    }
}

fn render_err(src: &str) -> Option<CardError> {
    match parse(src) {
        Err(e) => Some(e),
        Ok(p) => p.render(&p.store()).err(),
    }
}

#[test]
fn caps_on_size_depth_and_work_fail_the_card() {
    let big = format!(
        "root = Stack([TextContent(\"{}\")])",
        "y".repeat(MAX_SOURCE_BYTES)
    );
    assert_eq!(render_err(&big), Some(CardError::TooLarge));
    let deep = format!(
        "root = Stack([{}{}])",
        "[".repeat(MAX_DEPTH + 10),
        "]".repeat(MAX_DEPTH + 10)
    );
    assert_eq!(render_err(&deep), Some(CardError::TooDeep));
    let nest = |n: usize| {
        format!(
            "root = Stack([c0])\n{}c{n} = TextContent(\"bottom\")\n",
            (0..n)
                .map(|i| format!("c{i} = Card([c{}])\n", i + 1))
                .collect::<String>()
        )
    };
    assert_eq!(render_err(&nest(MAX_NESTING + 5)), Some(CardError::TooDeep));
    assert_eq!(
        render_err(&nest(MAX_NESTING - 2)),
        None,
        "a card nested to just under the cap is shown"
    );
    let many = format!(
        "root = Stack([{}])\n{}",
        (0..50)
            .map(|i| format!("g{i}"))
            .collect::<Vec<_>>()
            .join(", "),
        (0..50)
            .map(|i| format!(
                "g{i} = Card([{}])\n",
                (0..50)
                    .map(|j| format!("TextContent(\"t{i}-{j}\")"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .collect::<String>()
    );
    assert_eq!(render_err(&many), Some(CardError::TooMuch));
    let big_list = (0..2000)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let loops = format!(
        "root = Stack([Stack(@Each(big, \"a\", Stack(@Each(big, \"b\", TextContent(\"x\")))))])\nbig = [{big_list}]"
    );
    assert_eq!(render_err(&loops), Some(CardError::TooMuch));
    // One statement used four times in each of fourteen levels: 4^14 leaves to evaluate.
    let shared = format!(
        "root = Stack(a0)\n$n = 1\n{}a14 = TextContent(\"leaf \" + $n)\n",
        (0..14)
            .map(|i| format!("a{i} = [a{0}, a{0}, a{0}, a{0}]\n", i + 1))
            .collect::<String>()
    );
    assert_eq!(render_err(&shared), Some(CardError::TooMuch));
    // Without anything to evaluate, a smaller tree of the same shape draws its first
    // MAX_CHILDREN leaves.
    let literal = format!(
        "root = Stack(a0)\n{}a8 = TextContent(\"leaf\")\n",
        (0..8)
            .map(|i| format!("a{i} = [a{0}, a{0}, a{0}, a{0}]\n", i + 1))
            .collect::<String>()
    );
    assert_eq!(children(&card(&literal).2.root).len(), MAX_CHILDREN);
    let fanout = format!(
        "root = Stack(a0)\n{}a16 = TextContent(\"leaf\")\n",
        (0..16)
            .map(|i| format!("a{i} = [a{0}, a{0}, a{0}, nope]\n", i + 1))
            .collect::<String>()
    );
    assert_eq!(render_err(&fanout), Some(CardError::TooMuch));
    let text = format!(
        "root = Stack([{}])",
        (0..60)
            .map(|_| format!("TextContent(\"{}\")", "z".repeat(MAX_TEXT)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(text.len() < MAX_SOURCE_BYTES * 8);
    if text.len() <= MAX_SOURCE_BYTES {
        assert_eq!(render_err(&text), Some(CardError::TooMuch));
    }
    let doubling = format!(
        "root = Stack([{}])\ns0 = \"{}\"\n{}",
        (0..12)
            .map(|_| "TextContent(s24)")
            .collect::<Vec<_>>()
            .join(", "),
        "x".repeat(10),
        (1..=24)
            .map(|i| format!("s{i} = s{0} + s{0}\n", i - 1))
            .collect::<String>()
    );
    assert_eq!(
        render_err(&doubling),
        Some(CardError::TooMuch),
        "evaluating a string doubled on itself 24 times is over the work cap"
    );
}

#[test]
fn caps_on_lists_and_texts_cut_them_short() {
    let wide = format!(
        "root = Stack([{}])",
        (0..MAX_CHILDREN + 50)
            .map(|i| format!("TextContent(\"item {i}\")"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let (_, _, c) = card(&wide);
    assert_eq!(children(&c.root).len(), MAX_CHILDREN);
    let cols = (0..MAX_COLUMNS + 6)
        .map(|c| format!("Col(\"c{c}\", rows)"))
        .collect::<Vec<_>>()
        .join(", ");
    let rows = format!(
        "root = Stack([Table([{cols}])])\nrows = [{}]",
        (0..MAX_ROWS + 100)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let (_, _, c) = card(&rows);
    let Component::Table(t) = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(
        (t.columns.len(), t.rows.len(), t.rows[0].len()),
        (MAX_COLUMNS, MAX_ROWS, MAX_COLUMNS)
    );
    let n = MAX_POINTS + 500;
    let points = format!(
        "root = Stack([chart, scatter])\nchart = LineChart(labels, [{}])\nlabels = [{}]\nvalues = [{}]\nscatter = ScatterChart([ScatterSeries(\"pts\", [{}])])",
        (0..MAX_SERIES + 6)
            .map(|s| format!("Series(\"s{s}\", values)"))
            .collect::<Vec<_>>()
            .join(", "),
        (0..n)
            .map(|i| format!("\"p{i}\""))
            .collect::<Vec<_>>()
            .join(", "),
        (0..n)
            .map(|i| (i % 97).to_string())
            .collect::<Vec<_>>()
            .join(", "),
        (0..n)
            .map(|i| format!("Point({i}, {})", i % 13))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let (_, _, c) = card(&points);
    let Component::Chart(Chart::Bars(b)) = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(
        (b.labels.len(), b.series.len(), b.series[0].values.len()),
        (MAX_POINTS, MAX_SERIES, MAX_POINTS)
    );
    let Component::Chart(Chart::Scatter(s)) = &children(&c.root)[1] else {
        panic!()
    };
    assert_eq!(s.sets[0].points.len(), MAX_POINTS);
    let long = format!(
        "root = Stack([TextContent(\"{}\")])",
        "abcdefghij".repeat(MAX_TEXT / 5)
    );
    let (_, _, c) = card(&long);
    let Component::Text { markdown, .. } = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(markdown.chars().count(), MAX_TEXT);
    assert!(markdown.ends_with('…'));
    // $variables' defaults are evaluated once each, so this doubling is thirty steps; the
    // string stops at the computed-text cap, and what's drawn at the text cap.
    let doubling = format!(
        "root = Stack([TextContent($s30)])\n$s0 = \"xxxxxxxxxx\"\n{}",
        (1..=30)
            .map(|i| format!("$s{i} = $s{0} + $s{0}\n", i - 1))
            .collect::<String>()
    );
    let (p, _, c) = card(&doubling);
    assert_eq!(
        p.store().var("$s30").and_then(|v| v.as_str().map(str::len)),
        Some(super::MAX_EVAL_TEXT)
    );
    let Component::Text { markdown, .. } = &children(&c.root)[0] else {
        panic!()
    };
    assert_eq!(
        markdown.chars().count(),
        MAX_TEXT,
        "a string doubled thirty times stops at the caps"
    );
}

#[test]
fn hostile_shapes_stay_quick() {
    let started = std::time::Instant::now();
    // Thousands of statements, each referenced once.
    let n = 6_000;
    let many = format!(
        "root = Stack([{}])\n{}",
        (0..n)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(","),
        (0..n)
            .map(|i| format!("s{i} = TextContent(\"t\")\n"))
            .collect::<String>()
    );
    assert!(many.len() < MAX_SOURCE_BYTES);
    assert_eq!(children(&card(&many).2.root).len(), MAX_CHILDREN);
    // An object with tens of thousands of keys keeps MAX_KEYS of them.
    let keys = format!(
        "root = Stack([TextContent(\"\" + @Count(o.list))])\no = {{{}, list: [1, 2]}}",
        (0..20_000)
            .map(|i| format!("k{i}: 1"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(keys.len() < MAX_SOURCE_BYTES);
    assert_eq!(
        children(&card(&keys).2.root)[0],
        Component::Text {
            markdown: "0".into(),
            size: TextSize::Default
        },
        "the key past the cap isn't there"
    );
    // A $variable made of itself four times, twenty times over: 4^20 values by reference.
    let shared = format!(
        "root = Stack([b, Table([Col(\"v\", $v20)]), TextContent(\"\" + $v20)])\n$v0 = \"x\"\n{}b = Button(\"Go\", @ToAssistant(\"go\", $v20))",
        (1..=20)
            .map(|i| format!("$v{i} = [$v{0}, $v{0}, $v{0}, $v{0}]\n", i - 1))
            .collect::<String>()
    );
    let (p, mut store, c) = card(&shared);
    let Component::Button(b) = &children(&c.root)[0] else {
        panic!()
    };
    let sent = p.act(&b.action, &b.label, None, &mut store);
    assert!(matches!(&sent[..], [Effect::Send(t)] if t.len() < 600 * 1024));
    assert!(store.var("$v20").is_some());
    let Component::Text { markdown, .. } = &children(&c.root)[2] else {
        panic!()
    };
    assert_eq!(markdown.chars().count(), MAX_TEXT);
    // The same shape as literal lists: drawn as far as the caps.
    let literal = format!(
        "root = Stack(a0)\n{}a18 = TextContent(\"leaf\")\n",
        (0..18)
            .map(|i| format!("a{i} = [a{0}, a{0}, a{0}, a{0}]\n", i + 1))
            .collect::<String>()
    );
    assert!(matches!(
        render_err(&literal),
        None | Some(CardError::TooMuch)
    ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_card_can_be_read_on_another_thread() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Program>();
    send_sync::<Store>();
    send_sync::<Card>();
    send_sync::<Action>();
}

#[test]
fn deep_expressions_stop_at_the_cap_without_overflowing() {
    for depth in [20, MAX_DEPTH - 20, MAX_DEPTH - 2, MAX_DEPTH + 50] {
        let expr = format!("{}1{}", "1 + (".repeat(depth), ")".repeat(depth));
        let src = format!("root = Stack([TextContent(\"\" + ({expr}))])");
        match render_err(&src) {
            None => assert!(depth < MAX_DEPTH, "{depth}"),
            Some(e) => assert_eq!(e, CardError::TooDeep, "{depth}"),
        }
        let src = format!("root = Stack([TextContent(\"\" + $v)])\n$v = {expr}");
        assert!(
            matches!(render_err(&src), None | Some(CardError::TooDeep)),
            "{depth}"
        );
    }
    let (_, _, c) = card("root = Stack([TextContent(\"\" + (1 + (2 + (3))))])");
    assert_eq!(
        children(&c.root)[0],
        Component::Text {
            markdown: "6".into(),
            size: TextSize::Default
        }
    );
}

#[test]
fn an_empty_form_name_is_no_form() {
    let (p, mut store, _) = card("root = Stack([TextContent(\"x\")])");
    store.set_field(Some(""), None, "q", json!("hi"));
    assert_eq!(store.field(None, "q"), Some(json!("hi")));
    let sent = p.act(&Action::default_words(), "Go", Some(""), &mut store);
    assert_eq!(sent, [Effect::Send("Go\n\n```json\n{\n  \"form\": {\n    \"q\": {\n      \"value\": \"hi\"\n    }\n  }\n}\n```".into())]);
}

/// Reads, evaluates and draws a program the way the app would, checking the budget held: it
/// never ran past MAX_WORK without stopping the card, and every level it entered it left.
fn within_budget(src: &str) -> Result<Card, CardError> {
    if src.len() > MAX_SOURCE_BYTES {
        return Err(CardError::TooLarge);
    }
    let mut budget = Budget::new();
    let parsed = parse::parse(src, &mut budget);
    assert!(
        budget.work <= MAX_WORK || budget.failed == Some(CardError::TooMuch),
        "work {} without stopping: {src:?}",
        budget.work
    );
    assert!(
        budget.depth == 0 || budget.failed.is_some(),
        "depth {} left entered: {src:?}",
        budget.depth
    );
    let parsed = parsed?;
    let root = parsed.root.as_ref().ok_or(CardError::NoRoot)?;
    let store = Store::new(&parsed.state);
    let mut budget = Budget::new();
    let state = |name: &str| store.state(name);
    let evaluated = Eval::new(&mut budget, &state).root(Some(root), None);
    assert!(
        budget.work <= MAX_WORK || budget.failed == Some(CardError::TooMuch),
        "work {} without stopping: {src:?}",
        budget.work
    );
    assert!(
        budget.depth == 0 || budget.failed.is_some(),
        "depth {} left entered: {src:?}",
        budget.depth
    );
    if let Some(e) = budget.failed {
        return Err(e);
    }
    model::build(&evaluated.ok_or(CardError::NoRoot)?)
}

/// xorshift64: the same "random" programs on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.next() % items.len()]
    }
}

#[test]
fn random_cut_and_deep_programs_end_within_the_budget() {
    let mut samples: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "oui"))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    samples.sort();
    // Every prefix of every golden sample: a card cut short anywhere.
    for src in &samples {
        for end in (0..=src.len()).filter(|&i| src.is_char_boundary(i)) {
            let _ = within_budget(&src[..end]);
        }
    }
    // Programs of real tokens in random order, and the samples with random edits.
    let tokens = [
        "Stack(",
        "Card(",
        "TextContent(",
        "Table(",
        "Col(",
        "BarChart(",
        "Series(",
        "Form(",
        "FormControl(",
        "Input(",
        "Button(",
        "Buttons(",
        "Tabs(",
        "TabItem(",
        "Modal(",
        "Image(",
        "PieChart(",
        "ScatterChart(",
        "Point(",
        "Tag(",
        "Icon(",
        "CompositeCardBlock(",
        "CompositeCardItem(",
        "Unknown(",
        "@Each(",
        "@Count(",
        "@Filter(",
        "@Sort(",
        "@Sum(",
        "@Round(",
        "Action([",
        "@ToAssistant(",
        "@OpenUrl(",
        "@Set(",
        "@Reset(",
        "@Run(",
        "@Nope(",
        "Query(",
        "$x",
        "$y",
        "a",
        "b",
        "c",
        "root",
        " = ",
        "(",
        ")",
        "[",
        "]",
        "{",
        "}",
        ", ",
        ": ",
        "\"s\"",
        "'t'",
        "\"",
        "'",
        "\\",
        "1",
        "-2.5e3",
        "1e400",
        "true",
        "null",
        "+",
        "-",
        "*",
        "/",
        "%",
        "==",
        "!=",
        "<",
        ">=",
        "&&",
        "||",
        "!",
        "?",
        ".",
        "\n",
        "\n  ",
        "#",
        "//",
        "```",
        "\"https://example.com\"",
        "\"/api/files/content?path=a.png\"",
        "\"row\"",
        "\"stacked\"",
        "\"$x\"",
    ];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..1_500 {
        let mut src = String::from("root = Stack([");
        for _ in 0..rng.next() % 80 {
            src.push_str(rng.pick(&tokens));
        }
        for name in ["\na = ", "\nb = ", "\n$x = ", "\nc = "] {
            src.push_str(name);
            for _ in 0..rng.next() % 30 {
                src.push_str(rng.pick(&tokens));
            }
        }
        let _ = within_budget(&src);
    }
    for src in &samples {
        for _ in 0..20 {
            let mut s: Vec<char> = src.chars().collect();
            for _ in 0..1 + rng.next() % 6 {
                if s.is_empty() {
                    break;
                }
                let at = rng.next() % s.len();
                match rng.next() % 3 {
                    0 => drop(s.remove(at)),
                    1 => s.splice(at..at, rng.pick(&tokens).chars()).for_each(drop),
                    _ => {
                        let end = (at + rng.next() % 40).min(s.len());
                        let span: Vec<char> = s[at..end].to_vec();
                        s.splice(at..at, span).for_each(drop);
                    }
                }
            }
            let _ = within_budget(&s.into_iter().collect::<String>());
        }
    }
    // Nesting of every kind at and just past MAX_DEPTH: read, or stopped as too deep.
    for n in MAX_DEPTH - 3..=MAX_DEPTH + 3 {
        for src in deep_shapes(n) {
            match within_budget(&src) {
                Ok(_) | Err(CardError::TooDeep | CardError::NoRoot) => {}
                Err(e) => panic!("{e:?} at {n}: {}", &src[..src.len().min(120)]),
            }
            if n > MAX_DEPTH {
                assert!(
                    within_budget(&src).is_err(),
                    "nesting {n} was read: {}",
                    &src[..src.len().min(120)]
                );
            }
        }
    }
}

/// Programs that nest n deep in each way a program can: brackets, components (inline and by
/// reference), parentheses, chains, unary operators, ternaries, references, @Each, data in a
/// step's context, a @Set's value and $variables built from each other.
fn deep_shapes(n: usize) -> Vec<String> {
    let rep = |s: &str| s.repeat(n);
    let chain = |f: &dyn Fn(usize) -> String| (0..n).map(f).collect::<String>();
    vec![
        format!("root = Stack([{}{}])", rep("["), rep("]")),
        format!(
            "root = Stack([{}TextContent(\"x\"){}])",
            rep("Card(["),
            rep("])")
        ),
        format!(
            "root = Stack([c0])\n{}c{n} = TextContent(\"x\")",
            chain(&|i| format!("c{i} = Card([c{}])\n", i + 1))
        ),
        format!(
            "root = Stack([TextContent(\"\" + ({}1{}))])",
            rep("1 + ("),
            rep(")")
        ),
        format!(
            "root = Stack([TextContent(\"\" + $v)])\n$v = {}1{}",
            rep("1 + ("),
            rep(")")
        ),
        format!("root = Stack([TextContent(\"\" + {}true)])", rep("!")),
        format!("root = Stack([TextContent(\"\"{})])", rep(" + 1")),
        format!(
            "root = Stack([TextContent(\"\" + data{})])\ndata = {{a: 1}}",
            rep(".a")
        ),
        format!(
            "root = Stack([TextContent({}\"end\")])\n$t = false",
            rep("$t ? \"a\" : ")
        ),
        format!(
            "root = Stack([a0])\n{}a{n} = TextContent(\"x\")",
            chain(&|i| format!("a{i} = a{}\n", i + 1))
        ),
        format!(
            "root = Stack([TextContent(\"\" + a0)])\n{}a{n} = 1",
            chain(&|i| format!("a{i} = a{} + 1\n", i + 1))
        ),
        format!(
            "root = Stack({}TextContent(\"x\"){})",
            chain(&|i| format!("@Each([1], \"i{i}\", ")),
            rep(")")
        ),
        format!(
            "root = Stack([Button(\"go\", Action([@ToAssistant(\"x\", {}1{}), @Set($v, {}1{})]))])\n$v = 0",
            rep("{a: "),
            rep("}"),
            rep("["),
            rep("]")
        ),
        format!(
            "root = Stack([TextContent(\"\" + $v{n}), Button(\"go\", @ToAssistant(\"x\", $v{n})), Button(\"wrap\", @Set($v{n}, [$v{n}]))])\n$v0 = 1\n{}",
            (1..=n)
                .map(|i| format!("$v{i} = [$v{}]\n", i - 1))
                .collect::<String>()
        ),
    ]
}

#[test]
fn the_deepest_cards_fit_a_512_kb_stack() {
    // GPUI's background executor runs on Grand Central Dispatch's worker threads, which have
    // 512 KB stacks, so a card read in the background has to fit one: every shape of nesting,
    // at every depth up to past the caps, read, drawn and pressed. In a debug build the deepest
    // need 192 to 256 KB (nested components and nested @Each); release frames are smaller.
    let drawn = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let mut drawn = 0;
            for n in (1..=MAX_DEPTH + 10).rev() {
                for src in deep_shapes(n) {
                    let Ok(p) = parse(&src) else { continue };
                    let mut store = p.store();
                    let Ok(c) = p.render(&store) else { continue };
                    drawn += 1;
                    for comp in all(&c) {
                        if let Component::Button(b) = comp {
                            for _ in 0..3 {
                                let _ = p.act(&b.action, &b.label, None, &mut store);
                            }
                        }
                    }
                    let _ = (store.var("$v"), store.var(&format!("$v{n}")));
                    let _ = p.render(&store);
                }
            }
            drawn
        })
        .unwrap()
        .join()
        .expect("no overflow");
    assert!(drawn > 300, "only {drawn} of the deep cards were drawn");
}

#[test]
fn real_cards_stay_well_inside_the_depth_cap() {
    // The samples (the reference's examples and every component among them) reach about 20
    // levels reading and evaluating; MAX_DEPTH is chosen for the stack, and this keeps it at
    // least twice what real cards use.
    let mut names: Vec<_> = std::fs::read_dir(dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "oui"))
        .collect();
    names.sort();
    for path in names {
        let src = std::fs::read_to_string(&path).unwrap();
        let mut read = Budget::new();
        let Ok(p) = parse::parse(&src, &mut read) else {
            continue;
        };
        let store = Store::new(&p.state);
        let mut drawn = Budget::new();
        let state = |n: &str| store.state(n);
        let _ = Eval::new(&mut drawn, &state).root(p.root.as_ref(), None);
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            read.peak * 2 <= MAX_DEPTH && drawn.peak * 2 <= MAX_DEPTH,
            "{name}: {} reading, {} evaluating",
            read.peak,
            drawn.peak
        );
    }
}

#[test]
fn unknown_components_draw_nothing() {
    let (_, _, c) = card("root = Stack([Series(\"out of place\", [1]), TextContent(\"kept\")])");
    assert_eq!(children(&c.root)[0], Component::Unknown("Series".into()));
}
