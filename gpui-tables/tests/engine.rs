//! The engine: the fixtures' saved views worked out as a person would expect them, then every
//! filter operator, the sort and grouping rules, relations and what the engine does with views it
//! can't fully use.

use gpui_tables::engine::relation::{self, RowIndex};
use gpui_tables::engine::{self, Context, Group, GroupKey, Issue, Lookup, Part, query, time};
use gpui_tables::*;
use serde_json::{Value as Json, json};

fn docs(name: &str) -> Vec<Doc> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// 2026-11-15 12:00 UTC.
fn now() -> i64 {
    time::days_from_civil(2026, 11, 15) * engine::DAY_MS + 12 * 3_600_000
}

fn titles(rows: &[Doc], indexes: &[usize]) -> Vec<String> {
    indexes.iter().map(|&i| rows[i].title.clone()).collect()
}

fn view<'a>(db: &'a DatabaseFields, name: &str) -> &'a View {
    db.views.iter().find(|v| v.name == name).unwrap()
}

fn prop<'a>(db: &'a DatabaseFields, name: &str) -> &'a Property {
    db.properties.iter().find(|p| p.name == name).unwrap()
}

#[test]
fn the_people_table_sorts_by_name_and_leaves_out_the_deleted_row() {
    let docs = docs("people.json");
    let (db, rows) = (docs[0].database.as_ref().unwrap(), &docs[1..]);
    let out = query(db, view(db, "All people"), rows, &Context::new(now()));
    assert_eq!(out.issues, []);
    assert!(out.groups.is_none());
    assert_eq!(
        titles(rows, &out.rows),
        [
            "Ana Sofía Durán",
            "Imani Castell",
            "Jonah Whitfield-Arce",
            "Lena Varga",
            "Noor Ellison",
            "Rowan Pike",
            "Sami Halloran",
            "Theo Brandt"
        ]
    );
}

#[test]
fn a_tombstone_is_left_out_whatever_form_its_time_takes() {
    let docs = docs("people.json");
    let (db, mut rows) = (docs[0].database.as_ref().unwrap(), docs[1..].to_vec());
    let all = view(db, "All people");
    let count = |rows: &[Doc]| query(db, all, rows, &Context::new(now())).rows.len();
    let live = count(&rows);
    // The fixture's deleted row comes back once its time is cleared, as an undone delete does.
    let gone = rows
        .iter()
        .position(|r| r.title == "Duplicate of Rowan")
        .unwrap();
    assert_eq!(rows[gone].deleted, Some(1_793_613_600_000));
    rows[gone].deleted = None;
    assert_eq!(count(&rows), live + 1);
    // Ana, deleted at a time, at the epoch (purged), or by an older host's `true`.
    let ana = rows
        .iter()
        .position(|r| r.title == "Ana Sofía Durán")
        .unwrap();
    for deleted in [json!(now()), json!(0), json!(true)] {
        let mut json = serde_json::to_value(&rows[ana]).unwrap();
        json["deleted"] = deleted.clone();
        rows[ana] = serde_json::from_value(json).unwrap();
        assert!(rows[ana].is_deleted(), "{deleted}");
        let out = query(db, all, &rows, &Context::new(now()));
        assert_eq!(out.rows.len(), live, "{deleted}");
        assert!(!titles(&rows, &out.rows).contains(&"Ana Sofía Durán".to_owned()));
    }
}

#[test]
fn the_pipeline_board_groups_by_stage() {
    let docs = docs("people.json");
    let (db, rows) = (docs[0].database.as_ref().unwrap(), &docs[1..]);
    let board = view(db, "Pipeline");
    let out = query(db, board, rows, &Context::new(now()));
    let groups = out.groups.unwrap();
    let stage = prop(db, "Stage");
    let names: Vec<(String, Vec<String>)> = groups
        .iter()
        .map(|g| {
            let name = match &g.key {
                GroupKey::Empty => "No stage".to_owned(),
                GroupKey::Option(id) => stage.option(id).unwrap().name.clone(),
                other => panic!("{other:?}"),
            };
            (name, titles(rows, &g.rows))
        })
        .collect();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("No stage".into(), s(&[])),
            // Warmth descending; Jonah has none, so goes last.
            ("Lead".into(), s(&["Theo Brandt", "Jonah Whitfield-Arce"])),
            ("Warm".into(), s(&["Imani Castell", "Sami Halloran"])),
            // Rowan and Lena tie at 5 and keep the manual order.
            (
                "Active".into(),
                s(&["Rowan Pike", "Lena Varga", "Ana Sofía Durán"])
            ),
            ("Dormant".into(), s(&["Noor Ellison"])),
        ]
    );
    // Its collapsed group's key is the one the engine makes.
    assert_eq!(board.collapsed_groups, [groups[4].key.to_string()]);
}

#[test]
fn the_follow_up_list_filters_with_and_and_or() {
    let docs = docs("people.json");
    let (db, rows) = (docs[0].database.as_ref().unwrap(), &docs[1..]);
    let out = query(db, view(db, "Follow up"), rows, &Context::new(now()));
    assert_eq!(out.issues, []);
    // Checked, and last contacted before Oct 15 or never; oldest first, never last.
    assert_eq!(
        titles(rows, &out.rows),
        ["Sami Halloran", "Imani Castell", "Theo Brandt"]
    );
}

#[test]
fn the_timeline_groups_interactions_by_month() {
    let docs = docs("interactions.json");
    let (db, rows) = (docs[0].database.as_ref().unwrap(), &docs[1..]);
    let out = query(db, view(db, "Timeline"), rows, &Context::new(now()));
    let groups = out.groups.unwrap();
    let keys: Vec<String> = groups.iter().map(|g| g.key.to_string()).collect();
    assert_eq!(keys, ["month:2026-09", "month:2026-10", "month:2026-11"]);
    // Newest first inside each month.
    assert_eq!(
        titles(rows, &groups[2].rows),
        [
            "Portfolio review",
            "Catch-up on the new studio",
            "Quick thanks"
        ]
    );
    let open = query(db, view(db, "Open next steps"), rows, &Context::new(now()));
    assert_eq!(
        titles(rows, &open.rows),
        [
            "Referral for the platform role",
            "Pilot scoping",
            "Catch-up on the new studio"
        ]
    );
}

#[test]
fn companies_sort_by_value_with_the_empty_one_last_either_way() {
    let docs = docs("companies.json");
    let (db, rows) = (docs[0].database.as_ref().unwrap(), &docs[1..]);
    let mut v = view(db, "All companies").clone();
    let out = query(db, &v, rows, &Context::new(now()));
    assert_eq!(
        titles(rows, &out.rows),
        [
            "Lumen Freight",
            "Bluefin Robotics",
            "Kestrel Ceramics",
            "Cedar & Pine Studio"
        ]
    );
    v.sort[0].direction = Direction::Ascending;
    let out = query(db, &v, rows, &Context::new(now()));
    assert_eq!(
        titles(rows, &out.rows),
        [
            "Kestrel Ceramics",
            "Bluefin Robotics",
            "Lumen Freight",
            "Cedar & Pine Studio"
        ]
    );
}

// A small database of every type, for the operators.

struct Fixture {
    db: DatabaseFields,
    rows: Vec<Doc>,
}

const DAY: i64 = engine::DAY_MS;

fn day(y: i64, m: u32, d: u32) -> i64 {
    time::days_from_civil(y, m, d)
}

fn small() -> Fixture {
    let p = |id: &str, kind: PropertyType| Property::new(id, id, kind, "a0");
    let options = |names: &[&str]| PropertyConfig {
        options: Some(
            names
                .iter()
                .map(|n| SelectOption {
                    id: n.to_string(),
                    name: n.to_uppercase(),
                    color: Color::Default,
                })
                .collect(),
        ),
        ..PropertyConfig::default()
    };
    let db = DatabaseFields {
        properties: vec![
            p("name", PropertyType::Title),
            p("note", PropertyType::Text),
            p("n", PropertyType::Number),
            p("stage", PropertyType::Select).with_config(options(&["lead", "won", "lost"])),
            p("tags", PropertyType::MultiSelect).with_config(options(&["a", "b", "c"])),
            p("when", PropertyType::Date),
            p("done", PropertyType::Checkbox),
            p("site", PropertyType::Url),
            p("who", PropertyType::Person),
            p("rel", PropertyType::Relation).with_config(PropertyConfig {
                database_id: Some("other".into()),
                ..PropertyConfig::default()
            }),
            p("added", PropertyType::CreatedTime),
            p("edited", PropertyType::EditedTime),
            p("by", PropertyType::CreatedBy),
            p("editor", PropertyType::EditedBy),
        ],
        ..DatabaseFields::default()
    };
    let today = day(2026, 11, 15);
    let mut rows = vec![
        row(
            "r0",
            "Alpha",
            "a0",
            today - 40,
            &[
                ("note", Value::text("Met at the Ceramics Fair")),
                ("n", Value::Number(10.0)),
                ("stage", Value::text("lead")),
                ("tags", Value::ids(["a", "b"])),
                ("when", Value::date_on_day(today)),
                ("done", Value::Bool(true)),
                ("site", Value::text("https://alpha.example")),
                ("who", vec![Person::Owner].into()),
                ("rel", Value::ids(["x1", "x2"])),
            ],
        ),
        row(
            "r1",
            "beta",
            "a1",
            today - 3,
            &[
                ("note", Value::text("ÜBER café")),
                ("n", Value::Number(-2.5)),
                ("stage", Value::text("won")),
                ("tags", Value::ids(["c"])),
                (
                    "when",
                    Value::Date(DateValue {
                        // Nov 14 23:30 UTC: the 15th from UTC+1.
                        start: (today - 1) * DAY + 23 * 3_600_000 + 1_800_000,
                        end: None,
                        include_time: true,
                    }),
                ),
                ("done", Value::Bool(false)),
                (
                    "who",
                    vec![Person::agent("inbox-triage"), Person::Owner].into(),
                ),
                ("rel", Value::ids(["x2"])),
            ],
        ),
        row(
            "r2",
            "Gamma",
            "a2",
            today,
            &[
                ("n", Value::Number(10.0)),
                ("stage", Value::text("deleted-option")),
                ("tags", Value::ids(["gone"])),
                (
                    "when",
                    Value::Date(DateValue {
                        start: (today - 10) * DAY,
                        end: Some((today + 2) * DAY),
                        include_time: false,
                    }),
                ),
                ("who", Value::ids(Vec::<String>::new())),
            ],
        ),
        // Nothing set, and wrong shapes.
        row(
            "r3",
            "",
            "a3",
            today - 400,
            &[("n", Value::text("12")), ("stage", Value::Number(1.0))],
        ),
    ];
    // Who made each row and changed it last. r1 is older than `updatedBy`, so its editor is its
    // author; r3 has neither.
    for (r, author, editor) in [
        (0, Some(Person::Owner), Some(Person::agent("inbox-triage"))),
        (1, Some(Person::agent("career-planning")), None),
        (2, Some(Person::Owner), Some(Person::Owner)),
    ] {
        rows[r].author = author;
        rows[r].updated_by = editor;
    }
    Fixture { db, rows }
}

fn row(id: &str, title: &str, order: &str, created_day: i64, values: &[(&str, Value)]) -> Doc {
    let mut r = Doc::row(
        id,
        "db",
        title,
        order,
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    );
    r.created = created_day * DAY + 9 * 3_600_000;
    r.updated = r.created + 3_600_000;
    r
}

impl Fixture {
    /// The ids of rows passing one condition.
    fn pass(&self, property: &str, op: FilterOp, value: Json) -> Vec<&str> {
        self.pass_in(property, op, value, &Context::new(now()))
    }

    fn pass_in(&self, property: &str, op: FilterOp, value: Json, cx: &Context) -> Vec<&str> {
        let filter = Filter::condition(property, op, value);
        let (f, issues) = engine::CompiledFilter::new(Some(&filter), &self.db, cx);
        assert_eq!(issues, [], "{property} {filter:?}");
        self.rows
            .iter()
            .filter(|r| f.matches(r))
            .map(|r| r.id.as_str())
            .collect()
    }

    fn issues(&self, filter: Filter) -> Vec<Issue> {
        engine::CompiledFilter::new(Some(&filter), &self.db, &Context::new(now())).1
    }
}

#[test]
fn text_operators_ignore_case() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("name", O::Is, json!("ALPHA")), ["r0"]);
    assert_eq!(f.pass("name", O::IsNot, json!("alpha")), ["r1", "r2", "r3"]);
    assert_eq!(f.pass("name", O::Contains, json!("MM")), ["r2"]);
    assert_eq!(
        f.pass("name", O::DoesNotContain, json!("ph")),
        ["r1", "r2", "r3"]
    );
    assert_eq!(f.pass("name", O::StartsWith, json!("Be")), ["r1"]);
    assert_eq!(f.pass("name", O::IsEmpty, Json::Null), ["r3"]);
    assert_eq!(
        f.pass("name", O::IsNotEmpty, Json::Null),
        ["r0", "r1", "r2"]
    );
    // Beyond ASCII.
    assert_eq!(f.pass("note", O::Contains, json!("über")), ["r1"]);
    assert_eq!(f.pass("note", O::Contains, json!("CAFÉ")), ["r1"]);
    assert_eq!(f.pass("note", O::StartsWith, json!("met at")), ["r0"]);
    assert_eq!(f.pass("note", O::IsEmpty, Json::Null), ["r2", "r3"]);
    assert_eq!(f.pass("site", O::Contains, json!(".example")), ["r0"]);
    // Negatives match empty values.
    assert_eq!(
        f.pass("note", O::DoesNotContain, json!("fair")),
        ["r1", "r2", "r3"]
    );
}

#[test]
fn number_operators() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("n", O::Is, json!(10)), ["r0", "r2"]);
    assert_eq!(f.pass("n", O::IsNot, json!(10)), ["r1", "r3"]);
    assert_eq!(f.pass("n", O::Greater, json!(-2.5)), ["r0", "r2"]);
    assert_eq!(
        f.pass("n", O::GreaterOrEqual, json!(-2.5)),
        ["r0", "r1", "r2"]
    );
    assert_eq!(f.pass("n", O::Less, json!(10)), ["r1"]);
    assert_eq!(f.pass("n", O::LessOrEqual, json!(10)), ["r0", "r1", "r2"]);
    // A text "12" isn't a number.
    assert_eq!(f.pass("n", O::IsEmpty, Json::Null), ["r3"]);
    assert_eq!(f.pass("n", O::IsNotEmpty, Json::Null), ["r0", "r1", "r2"]);
}

#[test]
fn select_and_multi_select_operators() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("stage", O::Is, json!("lead")), ["r0"]);
    assert_eq!(f.pass("stage", O::IsNot, json!("lead")), ["r1", "r2", "r3"]);
    // A deleted option, and a number where an option should be, are empty.
    assert_eq!(f.pass("stage", O::IsEmpty, Json::Null), ["r2", "r3"]);
    assert_eq!(f.pass("stage", O::IsNotEmpty, Json::Null), ["r0", "r1"]);
    assert_eq!(f.pass("tags", O::Contains, json!("b")), ["r0"]);
    assert_eq!(
        f.pass("tags", O::DoesNotContain, json!("b")),
        ["r1", "r2", "r3"]
    );
    assert_eq!(f.pass("tags", O::IsEmpty, Json::Null), ["r2", "r3"]);
    assert_eq!(f.pass("tags", O::IsNotEmpty, Json::Null), ["r0", "r1"]);
}

#[test]
fn person_and_relation_operators() {
    let f = small();
    use FilterOp as O;
    assert_eq!(
        f.pass("who", O::Contains, json!({"kind": "owner"})),
        ["r0", "r1"]
    );
    assert_eq!(
        f.pass("who", O::Contains, json!("agent:inbox-triage")),
        ["r1"]
    );
    assert_eq!(
        f.pass(
            "who",
            O::Contains,
            json!({"kind": "agent", "agentId": "inbox-triage"})
        ),
        ["r1"]
    );
    assert_eq!(
        f.pass("who", O::DoesNotContain, json!("owner")),
        ["r2", "r3"]
    );
    assert_eq!(f.pass("who", O::IsEmpty, Json::Null), ["r2", "r3"]);
    assert_eq!(f.pass("who", O::IsNotEmpty, Json::Null), ["r0", "r1"]);
    assert_eq!(f.pass("rel", O::Contains, json!("x2")), ["r0", "r1"]);
    assert_eq!(
        f.pass("rel", O::DoesNotContain, json!("x1")),
        ["r1", "r2", "r3"]
    );
    assert_eq!(f.pass("rel", O::IsEmpty, Json::Null), ["r2", "r3"]);
    assert_eq!(f.pass("rel", O::IsNotEmpty, Json::Null), ["r0", "r1"]);
}

#[test]
fn created_by_and_edited_by_operators() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("by", O::Is, json!("owner")), ["r0", "r2"]);
    assert_eq!(
        f.pass("by", O::Contains, json!({"kind": "owner"})),
        ["r0", "r2"]
    );
    assert_eq!(
        f.pass(
            "by",
            O::Is,
            json!({"kind": "agent", "agentId": "career-planning"})
        ),
        ["r1"]
    );
    // The negatives match a doc with no author, as a person value's do.
    assert_eq!(f.pass("by", O::IsNot, json!("owner")), ["r1", "r3"]);
    assert_eq!(
        f.pass("by", O::DoesNotContain, json!("owner")),
        ["r1", "r3"]
    );
    assert_eq!(f.pass("by", O::IsEmpty, Json::Null), ["r3"]);
    assert_eq!(f.pass("by", O::IsNotEmpty, Json::Null), ["r0", "r1", "r2"]);

    assert_eq!(f.pass("editor", O::Is, json!("agent:inbox-triage")), ["r0"]);
    assert_eq!(f.pass("editor", O::Is, json!("owner")), ["r2"]);
    // r1 has no updatedBy: its editor is its author.
    assert_eq!(
        f.pass("editor", O::Is, json!("agent:career-planning")),
        ["r1"]
    );
    assert_eq!(
        f.pass("editor", O::IsNot, json!("owner")),
        ["r0", "r1", "r3"]
    );
    assert_eq!(f.pass("editor", O::IsEmpty, Json::Null), ["r3"]);

    // A stored value is ignored: the doc's fields win.
    let mut f = f;
    f.rows[3]
        .values
        .as_mut()
        .unwrap()
        .insert("by".into(), vec![Person::Owner].into());
    assert_eq!(f.pass("by", O::Is, json!("owner")), ["r0", "r2"]);

    let c = |p: &str, op: FilterOp, v: Json| Filter::condition(p, op, v);
    assert_eq!(
        f.issues(c("by", O::StartsWith, json!("o"))),
        [Issue::OpNotForType {
            property_id: "by".into(),
            op: O::StartsWith
        }]
    );
    assert_eq!(
        f.issues(c("editor", O::Before, json!("2026-11-15"))),
        [Issue::OpNotForType {
            property_id: "editor".into(),
            op: O::Before
        }]
    );
    assert_eq!(
        f.issues(c("by", O::Is, json!("agent:"))),
        [Issue::BadValue {
            property_id: "by".into(),
            op: O::Is
        }]
    );
}

#[test]
fn checkbox_operators() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("done", O::IsChecked, Json::Null), ["r0"]);
    // Missing is unchecked.
    assert_eq!(
        f.pass("done", O::IsNotChecked, Json::Null),
        ["r1", "r2", "r3"]
    );
    assert_eq!(f.pass("done", O::Is, json!(true)), ["r0"]);
    assert_eq!(f.pass("done", O::Is, json!(false)), ["r1", "r2", "r3"]);
}

#[test]
fn date_operators_count_whole_days() {
    let f = small();
    use FilterOp as O;
    // r0 is Nov 15, r1 Nov 14 23:30 UTC, r2 a range Nov 5 to Nov 17.
    assert_eq!(f.pass("when", O::Is, json!("2026-11-15")), ["r0", "r2"]);
    assert_eq!(f.pass("when", O::IsNot, json!("2026-11-15")), ["r1", "r3"]);
    assert_eq!(f.pass("when", O::Before, json!("2026-11-15")), ["r1", "r2"]);
    assert_eq!(f.pass("when", O::After, json!("2026-11-15")), ["r2"]);
    assert_eq!(
        f.pass("when", O::OnOrBefore, json!("2026-11-14")),
        ["r1", "r2"]
    );
    assert_eq!(
        f.pass("when", O::OnOrAfter, json!("2026-11-15")),
        ["r0", "r2"]
    );
    assert_eq!(f.pass("when", O::IsEmpty, Json::Null), ["r3"]);
    assert_eq!(
        f.pass("when", O::IsNotEmpty, Json::Null),
        ["r0", "r1", "r2"]
    );
    // The value as milliseconds or a date value.
    let nov15 = day(2026, 11, 15) * DAY;
    assert_eq!(f.pass("when", O::Is, json!(nov15 + 5)), ["r0", "r2"]);
    assert_eq!(
        f.pass("when", O::Is, json!({"start": nov15, "includeTime": false})),
        ["r0", "r2"]
    );
    // From UTC+1, r1's instant falls on the 15th; a date without a time stays the 15th.
    let east = Context::new(now()).with_offset(60);
    assert_eq!(
        f.pass_in("when", O::Is, json!("2026-11-15"), &east),
        ["r0", "r1", "r2"]
    );
}

#[test]
fn within_spans_count_from_today() {
    let f = small();
    use FilterOp as O;
    assert_eq!(f.pass("when", O::Within, json!("today")), ["r0", "r2"]);
    assert_eq!(
        f.pass("when", O::Within, json!("pastWeek")),
        ["r0", "r1", "r2"]
    );
    assert_eq!(f.pass("when", O::Within, json!("nextWeek")), ["r0", "r2"]);
    assert_eq!(f.pass("added", O::Within, json!("pastWeek")), ["r1", "r2"]);
    assert_eq!(f.pass("added", O::Within, json!("pastMonth")), ["r1", "r2"]);
    assert_eq!(
        f.pass("added", O::Within, json!("pastYear")),
        ["r0", "r1", "r2"]
    );
    assert_eq!(f.pass("added", O::Within, json!("nextMonth")), ["r2"]);
    assert_eq!(f.pass("added", O::Within, json!("nextYear")), ["r2"]);
    assert_eq!(
        f.pass("edited", O::Before, json!("2026-11-01")),
        ["r0", "r3"]
    );
    // The derived times are never empty.
    assert_eq!(f.pass("added", O::IsEmpty, Json::Null), Vec::<&str>::new());
    assert_eq!(
        engine::filter::within_days(&Within::PastMonth, day(2026, 3, 31)),
        Some((day(2026, 2, 28), day(2026, 3, 31)))
    );
}

#[test]
fn groups_combine_and_empty_groups_match_everything() {
    let f = small();
    let c = |p: &str, op: FilterOp, v: Json| Filter::condition(p, op, v);
    let filter = Filter::Or {
        or: vec![
            Filter::And {
                and: vec![
                    c("n", FilterOp::Is, json!(10)),
                    c("done", FilterOp::IsChecked, Json::Null),
                ],
            },
            c("name", FilterOp::Is, json!("beta")),
        ],
    };
    let (compiled, issues) =
        engine::CompiledFilter::new(Some(&filter), &f.db, &Context::new(now()));
    assert_eq!(issues, []);
    let ids: Vec<&str> = f
        .rows
        .iter()
        .filter(|r| compiled.matches(r))
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, ["r0", "r1"]);
    for empty in [Filter::And { and: vec![] }, Filter::Or { or: vec![] }] {
        let (compiled, _) = engine::CompiledFilter::new(Some(&empty), &f.db, &Context::new(now()));
        assert!(f.rows.iter().all(|r| compiled.matches(r)));
    }
}

#[test]
fn what_the_engine_cant_use_is_left_out_and_reported() {
    let f = small();
    let c = |p: &str, op: FilterOp, v: Json| Filter::condition(p, op, v);
    assert_eq!(
        f.issues(c("gone", FilterOp::Is, json!("x"))),
        [Issue::UnknownProperty {
            part: Part::Filter,
            property_id: "gone".into()
        }]
    );
    assert_eq!(
        f.issues(c("n", FilterOp::Contains, json!("1"))),
        [Issue::OpNotForType {
            property_id: "n".into(),
            op: FilterOp::Contains
        }]
    );
    assert_eq!(
        f.issues(c("n", FilterOp::Other("near".into()), json!(1))),
        [Issue::OpNotForType {
            property_id: "n".into(),
            op: FilterOp::Other("near".into())
        }]
    );
    assert_eq!(
        f.issues(c("n", FilterOp::Greater, json!("ten"))),
        [Issue::BadValue {
            property_id: "n".into(),
            op: FilterOp::Greater
        }]
    );
    assert_eq!(
        f.issues(c("when", FilterOp::Within, json!("someday"))),
        [Issue::BadValue {
            property_id: "when".into(),
            op: FilterOp::Within
        }]
    );
    assert_eq!(
        f.issues(c("when", FilterOp::Before, json!("2026-02-30"))),
        [Issue::BadValue {
            property_id: "when".into(),
            op: FilterOp::Before
        }]
    );
    // A condition still being written is left out quietly.
    assert_eq!(f.issues(c("name", FilterOp::Contains, Json::Null)), []);
    assert_eq!(f.issues(c("name", FilterOp::Contains, json!(""))), []);

    // Too deep: the deep part is left out, the rest still filters.
    let mut deep = c("name", FilterOp::Is, json!("nobody"));
    for _ in 0..4 {
        deep = Filter::And { and: vec![deep] };
    }
    let filter = Filter::And {
        and: vec![c("done", FilterOp::IsChecked, Json::Null), deep],
    };
    let (compiled, issues) =
        engine::CompiledFilter::new(Some(&filter), &f.db, &Context::new(now()));
    assert_eq!(issues, [Issue::FilterTooDeep { limit: 4 }]);
    assert_eq!(f.rows.iter().filter(|r| compiled.matches(r)).count(), 1);

    let wide = Filter::And {
        and: (0..150)
            .map(|_| c("name", FilterOp::IsNotEmpty, Json::Null))
            .collect(),
    };
    assert_eq!(f.issues(wide), [Issue::FilterTooBig { limit: 100 }]);

    // In a view: an unknown sort, too many sorts, an ungroupable property.
    let mut v = View::new("v", "All", ViewType::Table, "a0");
    v.sort = vec![Sort::ascending("gone")];
    v.sort.extend((0..10).map(|_| Sort::ascending("n")));
    v.group_by = Some("note".into());
    let out = query(&f.db, &v, &f.rows, &Context::new(now()));
    assert_eq!(
        out.issues,
        [
            Issue::TooManySorts { limit: 10 },
            Issue::UnknownProperty {
                part: Part::Sort,
                property_id: "gone".into()
            },
            Issue::CantGroupBy {
                property_id: "note".into()
            }
        ]
    );
    assert_eq!(out.rows.len(), 4);
    assert!(out.groups.is_none());
    v.group_by = Some("gone".into());
    let out = query(&f.db, &v, &f.rows, &Context::new(now()));
    assert_eq!(
        out.issues[2],
        Issue::UnknownProperty {
            part: Part::GroupBy,
            property_id: "gone".into()
        }
    );
}

fn sorted(f: &Fixture, sorts: Vec<Sort>) -> Vec<&str> {
    sorted_in(f, sorts, &Context::new(now()))
}

fn sorted_in<'f>(f: &'f Fixture, sorts: Vec<Sort>, cx: &Context) -> Vec<&'f str> {
    let mut v = View::new("v", "All", ViewType::Table, "a0");
    v.sort = sorts;
    let out = query(&f.db, &v, &f.rows, cx);
    assert_eq!(out.issues, []);
    out.rows.iter().map(|&i| f.rows[i].id.as_str()).collect()
}

#[test]
fn sorts_are_type_aware_with_empty_values_last() {
    let f = small();
    // Text ignores case; the empty title goes last both ways.
    assert_eq!(
        sorted(&f, vec![Sort::ascending("name")]),
        ["r0", "r1", "r2", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::descending("name")]),
        ["r2", "r1", "r0", "r3"]
    );
    // Numbers; r0 and r2 tie and keep the manual order.
    assert_eq!(
        sorted(&f, vec![Sort::ascending("n")]),
        ["r1", "r0", "r2", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::descending("n")]),
        ["r0", "r2", "r1", "r3"]
    );
    // Then by name to break the tie.
    assert_eq!(
        sorted(&f, vec![Sort::descending("n"), Sort::descending("name")]),
        ["r2", "r0", "r1", "r3"]
    );
    // A select by its options' order, not their names; a deleted option is empty.
    assert_eq!(
        sorted(&f, vec![Sort::descending("stage")]),
        ["r1", "r0", "r2", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::ascending("tags")]),
        ["r0", "r1", "r2", "r3"]
    );
    // Dates by their start.
    assert_eq!(
        sorted(&f, vec![Sort::ascending("when")]),
        ["r2", "r1", "r0", "r3"]
    );
    // Unchecked before checked; a missing checkbox is unchecked, not empty.
    assert_eq!(
        sorted(&f, vec![Sort::descending("done")]),
        ["r0", "r1", "r2", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::ascending("added")]),
        ["r3", "r0", "r1", "r2"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::descending("edited")]),
        ["r2", "r1", "r0", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::ascending("site")]),
        ["r0", "r1", "r2", "r3"]
    );
}

struct Names;

impl Lookup for Names {
    fn person_name(&self, p: &Person) -> Option<String> {
        Some(match p {
            Person::Owner => "Zoe".into(),
            Person::Agent(id) if id == "inbox-triage" => "Inbox".into(),
            _ => return None,
        })
    }

    fn row_title(&self, _: &str, row_id: &str) -> Option<String> {
        Some(match row_id {
            "x1" => "Zed Co".into(),
            "x2" => "Acme".into(),
            _ => return None,
        })
    }
}

#[test]
fn people_and_relations_sort_by_the_hosts_names() {
    let f = small();
    // Without names, by "Owner" and the agent's id: "inbox-triage, owner" before "owner".
    assert_eq!(
        sorted(&f, vec![Sort::ascending("who")]),
        ["r1", "r0", "r2", "r3"]
    );
    let cx = Context::new(now()).with_lookup(&Names);
    // "zoe" > "inbox, zoe".
    assert_eq!(
        sorted_in(&f, vec![Sort::ascending("who")], &cx),
        ["r1", "r0", "r2", "r3"]
    );
    // "zed co, acme" > "acme".
    assert_eq!(
        sorted_in(&f, vec![Sort::ascending("rel")], &cx),
        ["r1", "r0", "r2", "r3"]
    );
    assert_eq!(
        sorted_in(&f, vec![Sort::descending("rel")], &cx),
        ["r0", "r1", "r2", "r3"]
    );
}

/// The host's names for the created-by and edited-by sorts: the owner sorts by name, before an
/// agent named Scout.
struct Crew;

impl Lookup for Crew {
    fn person_name(&self, p: &Person) -> Option<String> {
        Some(match p {
            Person::Owner => "Avery Lin".into(),
            Person::Agent(id) if id == "career-planning" => "Scout".into(),
            _ => return None,
        })
    }
}

#[test]
fn created_by_and_edited_by_sort_by_the_hosts_names() {
    let f = small();
    // Without names: "career-planning" < "owner"; ties keep the manual order; no author last.
    assert_eq!(
        sorted(&f, vec![Sort::ascending("by")]),
        ["r1", "r0", "r2", "r3"]
    );
    assert_eq!(
        sorted(&f, vec![Sort::descending("by")]),
        ["r0", "r2", "r1", "r3"]
    );
    let cx = Context::new(now()).with_lookup(&Crew);
    // "avery lin" < "scout".
    assert_eq!(
        sorted_in(&f, vec![Sort::ascending("by")], &cx),
        ["r0", "r2", "r1", "r3"]
    );
    // "avery lin" (r2) < "inbox-triage" (r0, no name) < "scout" (r1, its author).
    assert_eq!(
        sorted_in(&f, vec![Sort::ascending("editor")], &cx),
        ["r2", "r0", "r1", "r3"]
    );
}

fn grouped(
    f: &Fixture,
    by: &str,
    date: Option<DateBucket>,
    cx: &Context,
) -> Vec<(String, Vec<String>)> {
    let mut v = View::new("v", "All", ViewType::Board, "a0");
    v.group_by = Some(by.into());
    v.group_date = date;
    let out = query(&f.db, &v, &f.rows, cx);
    assert_eq!(out.issues, []);
    let groups: Vec<Group> = out.groups.unwrap();
    groups
        .into_iter()
        .map(|g| {
            // Every key reads back.
            assert_eq!(GroupKey::parse(&g.key.to_string()), Some(g.key.clone()));
            (
                g.key.to_string(),
                g.rows.iter().map(|&i| f.rows[i].id.clone()).collect(),
            )
        })
        .collect()
}

fn g(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|x| x.to_string()).collect()))
        .collect()
}

#[test]
fn grouping_by_select_multi_select_checkbox_and_person() {
    let f = small();
    let cx = Context::new(now());
    assert_eq!(
        grouped(&f, "stage", None, &cx),
        g(&[
            ("empty", &["r2", "r3"]),
            ("option:lead", &["r0"]),
            ("option:won", &["r1"]),
            ("option:lost", &[])
        ])
    );
    assert_eq!(
        grouped(&f, "tags", None, &cx),
        g(&[
            ("empty", &["r2", "r3"]),
            ("option:a", &["r0"]),
            ("option:b", &["r0"]),
            ("option:c", &["r1"])
        ])
    );
    assert_eq!(
        grouped(&f, "done", None, &cx),
        g(&[("unchecked", &["r1", "r2", "r3"]), ("checked", &["r0"])])
    );
    assert_eq!(
        grouped(&f, "who", None, &cx),
        g(&[
            ("empty", &["r2", "r3"]),
            ("owner", &["r0", "r1"]),
            ("agent:inbox-triage", &["r1"])
        ])
    );
}

#[test]
fn grouping_by_created_by_and_edited_by() {
    let f = small();
    let cx = Context::new(now());
    assert_eq!(
        grouped(&f, "by", None, &cx),
        g(&[
            ("empty", &["r3"]),
            ("owner", &["r0", "r2"]),
            ("agent:career-planning", &["r1"])
        ])
    );
    assert_eq!(
        grouped(&f, "editor", None, &cx),
        g(&[
            ("empty", &["r3"]),
            ("owner", &["r2"]),
            ("agent:career-planning", &["r1"]),
            ("agent:inbox-triage", &["r0"])
        ])
    );
    // Agents go by the host's names: Scout after inbox-triage.
    let cx = cx.with_lookup(&Crew);
    let keys: Vec<String> = grouped(&f, "editor", None, &cx)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        keys,
        [
            "empty",
            "owner",
            "agent:inbox-triage",
            "agent:career-planning"
        ]
    );
}

#[test]
fn grouping_by_date() {
    let f = small();
    let cx = Context::new(now());
    assert_eq!(
        grouped(&f, "when", None, &cx),
        g(&[
            ("relative:past30", &["r2"]),
            ("relative:yesterday", &["r1"]),
            ("relative:today", &["r0"]),
            ("empty", &["r3"])
        ])
    );
    assert_eq!(
        grouped(&f, "when", Some(DateBucket::Day), &cx),
        g(&[
            ("day:2026-11-05", &["r2"]),
            ("day:2026-11-14", &["r1"]),
            ("day:2026-11-15", &["r0"]),
            ("empty", &["r3"])
        ])
    );
    // Nov 15, 2026 is a Sunday: its week starts Monday the 9th, or Sunday the 15th.
    assert_eq!(
        grouped(&f, "when", Some(DateBucket::Week), &cx),
        g(&[
            ("week:2026-11-02", &["r2"]),
            ("week:2026-11-09", &["r0", "r1"]),
            ("empty", &["r3"])
        ])
    );
    let sunday = Context {
        week_starts_monday: false,
        ..Context::new(now())
    };
    assert_eq!(
        grouped(&f, "when", Some(DateBucket::Week), &sunday),
        g(&[
            ("week:2026-11-01", &["r2"]),
            ("week:2026-11-08", &["r1"]),
            ("week:2026-11-15", &["r0"]),
            ("empty", &["r3"])
        ])
    );
    assert_eq!(
        grouped(&f, "added", Some(DateBucket::Month), &cx),
        g(&[
            ("month:2025-10", &["r3"]),
            ("month:2026-10", &["r0"]),
            ("month:2026-11", &["r1", "r2"])
        ])
    );
    assert_eq!(
        grouped(&f, "added", Some(DateBucket::Year), &cx),
        g(&[("year:2025", &["r3"]), ("year:2026", &["r0", "r1", "r2"])])
    );
}

#[test]
fn group_keys_parse_or_refuse() {
    for bad in [
        "",
        "option",
        "agent",
        "day:2026-13-01",
        "month:2026-00",
        "relative:soon",
        "nope:x",
    ] {
        assert_eq!(GroupKey::parse(bad), None, "{bad}");
    }
    assert_eq!(
        GroupKey::parse("option:a:b"),
        Some(GroupKey::Option("a:b".into()))
    );
}

// Relations.

#[test]
fn setting_a_two_way_relation_says_what_the_other_side_gains_and_loses() {
    let docs = docs("people.json");
    let interactions = crate::docs("interactions.json");
    let db = docs[0].database.as_ref().unwrap();
    let p = prop(db, "Interactions");
    let rowan = docs.iter().find(|d| d.title == "Rowan Pike").unwrap();
    let old = rowan.value(&p.id).unwrap().as_ids().unwrap().to_vec();
    let lena_call = interactions
        .iter()
        .find(|d| d.title == "Catch-up on the new studio")
        .unwrap();
    // Drop the first interaction, add Lena's call (twice: the repeat is dropped).
    let new = vec![old[1].clone(), lena_call.id.clone(), lena_call.id.clone()];
    let limits = Limits::default();
    let edits = relation::set(&rowan.id, p, &old, new, &limits);
    assert_eq!(edits.len(), 2);
    let Edit::RowValueChanged {
        value: Some(Value::Ids(ids)),
        ..
    } = &edits[0]
    else {
        panic!()
    };
    assert_eq!(ids, &[old[1].clone(), lena_call.id.clone()]);
    let Edit::RelationChanged {
        added,
        removed,
        database_id,
        back_property_id,
        ..
    } = &edits[1]
    else {
        panic!()
    };
    assert_eq!(added, std::slice::from_ref(&lena_call.id));
    assert_eq!(removed, &[old[0].clone()]);
    assert_eq!(database_id, &interactions[0].id);

    // The other side, from the host's current values.
    let index = RowIndex::new(&interactions[1..]);
    let back = relation::back_edits(
        &edits[1],
        |id| {
            index.get(id).map(|r| {
                r.value(back_property_id)
                    .and_then(Value::as_ids)
                    .unwrap_or(&[])
                    .to_vec()
            })
        },
        &limits,
    );
    assert_eq!(back.missing, Vec::<String>::new());
    assert_eq!(back.edits.len(), 2);
    for e in &back.edits {
        let Edit::RowValueChanged {
            row_id,
            property_id,
            value: Some(Value::Ids(ids)),
        } = e
        else {
            panic!()
        };
        assert_eq!(property_id, back_property_id);
        assert_eq!(ids.contains(&rowan.id), row_id == &lena_call.id);
    }
    // Setting it to what it is: nothing.
    assert_eq!(relation::set(&rowan.id, p, &old, old.clone(), &limits), []);
}

#[test]
fn back_edits_skip_missing_rows_full_rows_and_rows_already_right() {
    let edit = Edit::RelationChanged {
        row_id: "me".into(),
        property_id: "p".into(),
        database_id: "db".into(),
        back_property_id: "q".into(),
        added: vec!["gone".into(), "full".into(), "has".into(), "new".into()],
        removed: vec!["lacks".into(), "had".into()],
    };
    let limits = Limits {
        relation_values: 3,
        ..Limits::default()
    };
    let out = relation::back_edits(
        &edit,
        |id| match id {
            "gone" => None,
            "full" => Some(vec!["a".into(), "b".into(), "c".into()]),
            "has" | "had" => Some(vec!["me".into()]),
            _ => Some(vec![]),
        },
        &limits,
    );
    assert_eq!(out.missing, ["gone"]);
    assert_eq!(out.full, ["full"]);
    let changed: Vec<(&str, Vec<String>)> = out
        .edits
        .iter()
        .map(|e| match e {
            Edit::RowValueChanged {
                row_id,
                value: Some(Value::Ids(ids)),
                ..
            } => (row_id.as_str(), ids.clone()),
            _ => panic!(),
        })
        .collect();
    assert_eq!(changed, [("new", vec!["me".to_owned()]), ("had", vec![])]);
}

#[test]
fn a_one_way_relation_has_no_back_side_and_a_new_pair_points_both_ways() {
    let one_way =
        Property::new("r", "Refs", PropertyType::Relation, "a0").with_config(PropertyConfig {
            database_id: Some("b".into()),
            ..PropertyConfig::default()
        });
    let edits = relation::set("me", &one_way, &[], vec!["x".into()], &Limits::default());
    assert_eq!(edits.len(), 1);
    // Not a relation: nothing.
    let text = Property::new("t", "Text", PropertyType::Text, "a0");
    assert_eq!(
        relation::set("me", &text, &[], vec!["x".into()], &Limits::default()),
        []
    );

    let (a, b) = relation::two_way_pair("db-a", "Company", "a5", "db-b", "People", "a6");
    let da = DatabaseFields {
        properties: vec![a.clone()],
        ..DatabaseFields::default()
    };
    let db = DatabaseFields {
        properties: vec![b.clone()],
        ..DatabaseFields::default()
    };
    assert_eq!(relation::check_two_way("db-a", &da, "db-b", &db), []);
    assert_eq!(relation::check_two_way("db-b", &db, "db-a", &da), []);
    assert_eq!(
        relation::check_two_way("db-a", &da, "db-b", &DatabaseFields::default()).len(),
        1
    );
    assert_eq!(a.relation().unwrap().back_property_id, Some(b.id.as_str()));
}

#[test]
fn diff_keeps_order_and_drops_repeats() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let (added, removed) =
        relation::diff(&s(&["a", "b", "b", "c"]), &s(&["c", "d", "d", "a", "e"]));
    assert_eq!(added, s(&["d", "e"]));
    assert_eq!(removed, s(&["b"]));
}

// Hostile data: every property given every shape of value, every operator given every value,
// sorted and grouped every way. Nothing may panic.

fn weird_values() -> Vec<Json> {
    vec![
        Json::Null,
        json!(true),
        json!(0),
        json!(-1.5),
        json!(1e300),
        json!(i64::MIN),
        json!(i64::MAX),
        json!(u64::MAX),
        json!(""),
        json!("lead"),
        json!("agent:"),
        json!("owner"),
        json!("2026-11-15"),
        json!("today"),
        json!("ﷺ𝒜\u{0}\u{202e}"),
        json!("x".repeat(20_000)),
        json!([]),
        json!(["a", "a", "gone"]),
        json!([1, 2]),
        json!([{"kind": "owner"}, {"kind": "agent"}]),
        json!([{"kind": "agent", "agentId": "inbox-triage"}]),
        json!({"start": i64::MIN, "end": i64::MAX, "includeTime": true}),
        json!({"start": i64::MAX, "includeTime": false}),
        json!({"start": 0, "end": -1}),
        json!({"kind": "owner"}),
        json!({"and": []}),
    ]
}

#[test]
fn hostile_values_never_panic() {
    let f = small();
    let values = weird_values();
    let mut rows = Vec::new();
    for (i, v) in values.iter().enumerate() {
        let mut map = Values::new();
        for (j, p) in f.db.properties.iter().enumerate() {
            map.insert(
                p.id.clone(),
                Value::from_json(values[(i + j) % values.len()].clone()),
            );
        }
        let mut r = Doc::row(
            format!("h{i}"),
            "db",
            v.as_str().unwrap_or("").to_owned(),
            "",
            map,
        );
        r.created = [i64::MIN, 0, i64::MAX][i % 3];
        r.updated = -r.created.saturating_add(1);
        let someone = [
            None,
            Some(Person::Owner),
            Some(Person::agent("")),
            Some(Person::Other(json!({"kind": "robot"}))),
        ];
        r.author = someone[i % 4].clone();
        r.updated_by = someone[(i / 4) % 4].clone();
        rows.push(r);
    }
    rows.extend(f.rows.iter().cloned());
    let limits = Limits::default();
    for r in &rows {
        let _ = limits.check_row(&f.db, r);
        for p in &f.db.properties {
            if let Some(v) = r.value(&p.id) {
                let _ = limits.clamp_value(p, v.clone());
            }
        }
    }
    for cx in [
        Context::new(now()),
        Context::new(i64::MIN).with_offset(i32::MIN),
        Context::new(i64::MAX).with_offset(i32::MAX),
    ] {
        for p in &f.db.properties {
            for op in FilterOp::ALL {
                for v in &values {
                    let mut view = View::new("v", "All", ViewType::Board, "a0");
                    view.filter = Some(Filter::condition(&p.id, op.clone(), v.clone()));
                    let _ = query(&f.db, &view, &rows, &cx);
                }
            }
            for bucket in DateBucket::ALL {
                let mut view = View::new("v", "All", ViewType::Board, "a0");
                view.sort = vec![Sort::ascending(&p.id), Sort::descending("name")];
                view.group_by = Some(p.id.clone());
                view.group_date = Some(bucket.clone());
                let out = query(&f.db, &view, &rows, &cx);
                assert_eq!(out.rows.len(), rows.len());
            }
        }
    }
}

/// People with Stage, a select, changed to text: its rows keep their option ids, and its cells
/// show the options' names.
fn people_with_stage_as_text() -> (DatabaseFields, Vec<Doc>, Property) {
    let docs = docs("people.json");
    let mut db = docs[0].database.clone().unwrap();
    let stage = db
        .properties
        .iter_mut()
        .find(|p| p.name == "Stage")
        .unwrap();
    *stage = stage.retyped(PropertyType::Text);
    let stage = stage.clone();
    (db, docs[1..].to_vec(), stage)
}

/// What a row's Stage cell shows, as text.
fn shown(stage: &Property, row: &Doc) -> String {
    let id = row
        .value(&stage.id)
        .and_then(Value::as_str)
        .unwrap_or_default();
    stage.shown_text(id).to_owned()
}

#[test]
fn a_select_changed_to_text_filters_by_the_names_it_shows() {
    let (db, rows, stage) = people_with_stage_as_text();
    let live: Vec<&Doc> = rows.iter().filter(|r| !r.is_deleted()).collect();
    let cx = Context::new(now());
    for (op, needle, want) in [
        (
            FilterOp::Contains,
            "lea",
            &(|s: &str| s.contains("Lead")) as &dyn Fn(&str) -> bool,
        ),
        (FilterOp::Is, "lead", &|s: &str| s == "Lead"),
        (FilterOp::StartsWith, "Act", &|s: &str| {
            s.starts_with("Active")
        }),
        (FilterOp::DoesNotContain, "lead", &|s: &str| {
            !s.contains("Lead")
        }),
        (FilterOp::IsNot, "warm", &|s: &str| s != "Warm"),
    ] {
        let filter = Filter::condition(&stage.id, op.clone(), json!(needle));
        let (f, issues) = engine::CompiledFilter::new(Some(&filter), &db, &cx);
        assert_eq!(issues, []);
        let got: Vec<&str> = live
            .iter()
            .filter(|r| f.matches(r))
            .map(|r| r.title.as_str())
            .collect();
        let expected: Vec<&str> = live
            .iter()
            .filter(|r| want(&shown(&stage, r)))
            .map(|r| r.title.as_str())
            .collect();
        assert!(
            !expected.is_empty() && expected.len() < live.len(),
            "{op:?}"
        );
        assert_eq!(got, expected, "{op:?} {needle}");
    }
}

#[test]
fn a_select_changed_to_text_sorts_by_the_names_it_shows() {
    let (mut db, rows, stage) = people_with_stage_as_text();
    let mut v = view(&db, "All people").clone();
    v.filter = None;
    for direction in [Direction::Ascending, Direction::Descending] {
        let descending = direction == Direction::Descending;
        v.sort = vec![Sort {
            property_id: stage.id.clone(),
            direction,
        }];
        db.views = vec![v.clone()];
        let out = query(&db, &v, &rows, &Context::new(now()));
        assert_eq!(out.issues, []);
        let names: Vec<String> = out.rows.iter().map(|&i| shown(&stage, &rows[i])).collect();
        let mut sorted = names.clone();
        sorted.sort();
        if descending {
            sorted.reverse();
        }
        // Active, Dormant, Lead, Warm: the ids' order (Active, Lead, Dormant, Warm) differs.
        assert_eq!(names, sorted, "descending: {descending}");
    }
}
