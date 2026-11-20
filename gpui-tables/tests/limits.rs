//! The limit vectors in tests/fixtures/limits (see its README), which a host's server can test
//! against too: every valid case has no problems, and every invalid case has exactly the problems it
//! lists, written as `Problem`'s Display writes them, in any order.

use gpui_tables::{DatabaseFields, Doc, Limits};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    /// For a row: the database doc it's in.
    database: Option<Doc>,
    doc: Doc,
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
    let schema: Option<&DatabaseFields> = case.database.as_ref().and_then(|d| d.database.as_ref());
    let mut got: Vec<String> = Limits::default()
        .check_doc(&case.doc, schema)
        .iter()
        .map(ToString::to_string)
        .collect();
    got.sort();
    got
}

#[test]
fn valid_docs_have_no_problems() {
    let cases = cases("valid.json");
    assert!(cases.len() >= 8);
    for case in &cases {
        assert_eq!(problems(case), Vec::<String>::new(), "{}", case.name);
    }
}

#[test]
fn invalid_docs_have_exactly_their_problems() {
    let cases = cases("invalid.json");
    assert!(cases.len() >= 60);
    let mut failures = Vec::new();
    for case in &cases {
        assert!(!case.problems.is_empty(), "{}", case.name);
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
fn every_limit_has_a_case_just_over_it() {
    // Each limit's number appears as "more than N" in some invalid case's problems. (Several
    // limits share a number; the cases' names say which is which.)
    let l = Limits::default();
    let text: String = cases("invalid.json")
        .iter()
        .flat_map(|c| c.problems.clone())
        .collect::<Vec<_>>()
        .join("\n");
    for (name, n) in [
        ("properties", l.properties),
        ("views", l.views),
        ("sorts", l.sorts),
        ("options", l.options),
        ("multi_select_values", l.multi_select_values),
        ("relation_values", l.relation_values),
        ("person_values", l.person_values),
        ("collapsed_groups", l.collapsed_groups),
        ("title_chars", l.title_chars),
        ("text_chars", l.text_chars),
        ("url_chars", l.url_chars),
        ("email_chars", l.email_chars),
        ("phone_chars", l.phone_chars),
        ("option_name_chars", l.option_name_chars),
        ("name_chars", l.name_chars),
        ("id_chars", l.id_chars),
        ("order_chars", l.order_chars),
        ("filter_depth", l.filter_depth),
        ("filter_nodes", l.filter_nodes),
    ] {
        assert!(
            text.contains(&format!("more than {n}\n")) || text.ends_with(&format!("more than {n}")),
            "{name}"
        );
    }
}

#[test]
fn every_icon_check_has_a_case() {
    let text: Vec<String> = cases("invalid.json")
        .iter()
        .flat_map(|c| c.problems.clone())
        .collect();
    for message in [
        "icon: 2 graphemes is more than 1",
        "icon: 18 bytes is more than 16",
        "icon: an empty string",
        "icon: not asset: and 64 lowercase hex characters",
        "icon: not a string or null",
    ] {
        assert!(text.iter().any(|t| t == message), "{message}");
    }
}

/// A CRM fixture: its database doc and rows.
fn crm(name: &str) -> (Doc, Vec<Doc>) {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let mut docs: Vec<Doc> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rows = docs.split_off(1);
    (docs.remove(0), rows)
}

#[test]
fn rows_stay_valid_when_a_select_and_a_multi_select_swap_types() {
    use gpui_tables::PropertyType as T;
    let limits = Limits::default();
    let mut swapped = 0;
    for name in ["people", "companies", "interactions"] {
        let (db, rows) = crm(name);
        let fields = db.database.as_ref().unwrap();
        for row in &rows {
            assert_eq!(
                limits.check_doc(row, Some(fields)),
                [],
                "{name}: {}",
                row.title
            );
        }
        // Every select made a multi-select and every multi-select a select, then back.
        let mut there = fields.clone();
        for p in &mut there.properties {
            *p = match p.kind {
                T::Select => p.retyped(T::MultiSelect),
                T::MultiSelect => p.retyped(T::Select),
                _ => continue,
            };
            swapped += 1;
        }
        let mut back = there.clone();
        for p in &mut back.properties {
            *p = match p.kind {
                T::Select => p.retyped(T::MultiSelect),
                T::MultiSelect => p.retyped(T::Select),
                _ => continue,
            };
        }
        for fields in [&there, &back] {
            for row in &rows {
                assert_eq!(
                    limits.check_doc(row, Some(fields)),
                    [],
                    "{name}: {}",
                    row.title
                );
            }
        }
    }
    assert_eq!(swapped, 4, "Stage, Tags, Industry and Kind");
}
