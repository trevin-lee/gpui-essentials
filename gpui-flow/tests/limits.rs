//! The limit vectors in tests/fixtures/limits (see its README), which the server tests against
//! too: every valid case has no problems, and every invalid case has exactly the problems it
//! lists, written as `Problem`'s Display writes them, in any order.

use gpui_flow::json::{Document, Op};
use gpui_flow::limits::Limits;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    name: String,
    #[serde(default)]
    limits: Limits,
    doc: Option<Value>,
    ops: Option<Vec<Op>>,
    problems: Vec<String>,
}

fn cases(file: &str) -> Vec<Case> {
    let path = format!(
        "{}/tests/fixtures/limits/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn problems(case: &Case) -> Vec<String> {
    let found = match (&case.doc, &case.ops) {
        // As stored, not read with `from_json`, which would mend the ids being checked.
        (Some(doc), _) => {
            let doc: Document = serde_json::from_value(doc.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            case.limits.check(&doc)
        }
        (None, Some(ops)) => case.limits.check_batch(ops),
        _ => panic!("{}: neither a doc nor ops", case.name),
    };
    let mut found: Vec<String> = found.iter().map(ToString::to_string).collect();
    found.sort();
    found
}

fn run(file: &str, at_least: usize, valid: bool) {
    let cases = cases(file);
    assert!(cases.len() >= at_least, "{file}: {}", cases.len());
    let mut failures = Vec::new();
    for case in &cases {
        assert_eq!(case.problems.is_empty(), valid, "{}", case.name);
        let mut want = case.problems.clone();
        want.sort();
        let got = problems(case);
        if got != want {
            failures.push(format!("{}:\n  want {want:?}\n  got  {got:?}", case.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn valid_documents_have_no_problems() {
    run("valid.json", 20, true);
}

#[test]
fn invalid_documents_have_exactly_their_problems() {
    run("invalid.json", 48, false);
}

#[test]
fn batches_have_exactly_their_problems() {
    let cases = cases("batches.json");
    assert!(cases.len() >= 10);
    for case in &cases {
        let mut want = case.problems.clone();
        want.sort();
        assert_eq!(problems(case), want, "{}", case.name);
    }
}

#[test]
fn the_defaults_are_the_documented_ones() {
    let l = Limits::default();
    assert_eq!(
        (
            l.nodes,
            l.edges,
            l.ops,
            l.id_chars,
            l.text_chars,
            l.url_chars,
            l.depth
        ),
        (10_000, 20_000, 30_000, 128, 10_000, 2_048, 16)
    );
    assert_eq!((l.node_bytes, l.edge_bytes), (1_048_576, 65_536));
}
