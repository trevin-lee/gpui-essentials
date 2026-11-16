//! The model against the fixtures (a fictional personal CRM: People, Companies and Interactions
//! databases related to each other, the page that embeds one, and the events a view emits):
//! every file reads into the typed model and writes back as the same JSON, the values read as
//! the types they should, and everything is within the limits. Then the limits themselves:
//! over-limit input is reported or clamped, never a panic.

use gpui_tables::engine::relation::{self, RowIndex};
use gpui_tables::*;
use serde_json::{Value as Json, json};

fn fixture(name: &str) -> Json {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn docs(name: &str) -> Vec<Doc> {
    serde_json::from_value(fixture(name)).unwrap()
}

fn prop<'a>(db: &'a Doc, name: &str) -> &'a Property {
    let fields = db.database.as_ref().unwrap();
    fields.properties.iter().find(|p| p.name == name).unwrap()
}

fn row<'a>(docs: &'a [Doc], title: &str) -> &'a Doc {
    docs.iter().find(|d| d.title == title).unwrap()
}

#[test]
fn every_fixture_round_trips() {
    for name in [
        "people.json",
        "companies.json",
        "interactions.json",
        "crm-page.json",
    ] {
        let json = fixture(name);
        let docs: Vec<Doc> = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&docs).unwrap(), json, "{name}");
    }
    let json = fixture("events.json");
    let events: Vec<TableEvent> = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(events.len(), 15);
    assert_eq!(serde_json::to_value(&events).unwrap(), json);
}

#[test]
fn docs_read_as_databases_and_rows() {
    let people = docs("people.json");
    let db = &people[0];
    assert_eq!(db.kind, DocKind::Database);
    assert!(db.values.is_none());
    let fields = db.database.as_ref().unwrap();
    assert_eq!(fields.properties.len(), 15);
    assert_eq!(fields.views.len(), 4);
    assert_eq!(fields.title_property().unwrap().name, "Name");
    assert_eq!(
        fields
            .views_in_order()
            .iter()
            .map(|v| v.kind.clone())
            .collect::<Vec<_>>(),
        [
            ViewType::Table,
            ViewType::Board,
            ViewType::List,
            ViewType::List
        ]
    );
    assert_eq!(db.author, Some(Person::Owner));
    assert_eq!(db.rev, Some(14));
    assert!(db.folder_id.is_some() && db.parent_doc_id.is_none());

    // Rows are pages under the database.
    for r in &people[1..] {
        assert_eq!(r.kind, DocKind::Page);
        assert_eq!(r.parent_doc_id.as_deref(), Some(db.id.as_str()));
        assert!(r.database.is_none() && r.values.is_some());
    }
    assert!(row(&people, "Duplicate of Rowan").deleted);

    // An inline database: under a page, embedded by one of its blocks.
    let interactions = docs("interactions.json");
    let crm = &docs("crm-page.json")[0];
    assert_eq!(
        interactions[0].parent_doc_id.as_deref(),
        Some(crm.id.as_str())
    );
    assert!(interactions[0].parent_object_id.is_some());
    assert!(interactions[0].folder_id.is_none());
}

#[test]
fn values_read_as_their_types() {
    let people = docs("people.json");
    let db = &people[0];
    let imani = row(&people, "Imani Castell");

    let stage = prop(db, "Stage");
    let warm = stage.options().iter().find(|o| o.name == "Warm").unwrap();
    assert_eq!(imani.value(&stage.id), Some(&Value::text(&warm.id)));
    assert_eq!(warm.color, Color::Yellow);

    let tags = imani.value(&prop(db, "Tags").id).unwrap().as_ids().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(
        imani.value(&prop(db, "Warmth").id).unwrap().as_number(),
        Some(4.0)
    );
    assert_eq!(
        imani.value(&prop(db, "Follow up").id).unwrap().as_bool(),
        Some(true)
    );
    let date = imani
        .value(&prop(db, "Last contacted").id)
        .unwrap()
        .as_date()
        .unwrap();
    assert!(!date.include_time && date.end.is_none());
    assert_eq!(
        engine::time::format_ymd(date.start / engine::DAY_MS),
        "2026-10-02"
    );
    assert_eq!(
        imani
            .value(&prop(db, "Owner").id)
            .unwrap()
            .as_people()
            .unwrap(),
        [Person::Owner, Person::agent("career-planning")]
    );
    assert_eq!(
        imani.value(&prop(db, "Email").id).unwrap().as_str(),
        Some("imani@bluefin-robotics.example")
    );

    // An empty person list reads as ids but means no people.
    let sami = row(&people, "Sami Halloran");
    let owner = sami.value(&prop(db, "Owner").id).unwrap();
    assert_eq!(owner.as_people(), Some(&[][..]));
    assert!(owner.is_empty() && owner.fits(&PropertyType::Person));

    // Configs read as their types.
    let companies = docs("companies.json");
    let value = prop(&companies[0], "Annual value");
    assert_eq!(value.config.format, Some(NumberFormat::Currency));
    assert_eq!(value.config.currency.as_deref(), Some("USD"));
    let company = prop(db, "Company").relation().unwrap();
    assert!(company.two_way);
    assert_eq!(company.database_id, companies[0].id);
}

#[test]
fn the_fixtures_are_within_the_limits() {
    let limits = Limits::default();
    for name in ["people.json", "companies.json", "interactions.json"] {
        let docs = docs(name);
        let fields = docs[0].database.as_ref().unwrap();
        assert_eq!(limits.check_database(fields), [], "{name}");
        for r in &docs[1..] {
            assert_eq!(limits.check_row(fields, r), [], "{name}: {}", r.title);
        }
    }
}

#[test]
fn the_fixtures_relations_agree_on_both_sides() {
    let sets = [
        docs("people.json"),
        docs("companies.json"),
        docs("interactions.json"),
    ];
    for a in &sets {
        for b in &sets {
            let (fa, fb) = (
                a[0].database.as_ref().unwrap(),
                b[0].database.as_ref().unwrap(),
            );
            assert_eq!(relation::check_two_way(&a[0].id, fa, &b[0].id, fb), []);
        }
    }
    // Every id on one side has the row listed back on the other.
    for a in &sets {
        let fa = a[0].database.as_ref().unwrap();
        for p in fa.properties.iter().filter(|p| p.relation().is_some()) {
            let t = p.relation().unwrap();
            let b = sets.iter().find(|s| s[0].id == t.database_id).unwrap();
            let index = RowIndex::new(&b[1..]);
            for r in a[1..].iter().filter(|r| !r.deleted) {
                let ids = r.value(&p.id).and_then(Value::as_ids).unwrap_or(&[]);
                assert_eq!(index.dangling(ids), Vec::<&str>::new());
                for other in index.resolve(ids) {
                    let back = other.value(t.back_property_id.unwrap()).unwrap();
                    assert!(
                        back.as_ids().unwrap().contains(&r.id),
                        "{} ↔ {}",
                        r.title,
                        other.title
                    );
                }
            }
        }
    }
}

#[test]
fn unknown_fields_and_kinds_survive_a_round_trip() {
    let json = json!({
        "id": "0b5e7d1c-0000-4000-8000-000000000001",
        "kind": "whiteboard",
        "title": "From a newer host",
        "order": "a0",
        "folderId": null,
        "parentDocId": null,
        "parentObjectId": null,
        "cover": {"emoji": "🗂"},
        "favorite": false,
        "agents": {"anything": [1, 2]},
        "author": {"kind": "team", "teamId": "t1"},
        "updatedBy": {"kind": "agent", "agentId": "inbox-triage"},
        "created": 1,
        "updated": 2,
        "rev": null,
        "seq": null,
        "deleted": false,
        "values": {
            "a": {"start": 0, "tz": "Europe/Paris"},
            "b": [1, "x"],
            "c": [{"kind": "owner"}, {"kind": "robot"}],
            "d": 1.5,
            "e": null
        },
        "database": {"properties": [{
            "id": "p", "name": "Mood", "type": "emoji", "config": {"palette": "warm"}, "order": "a0",
            "description": "a newer field"
        }], "views": [{
            "id": "v", "name": "Map", "type": "map", "order": "a0", "sort": [], "filter": null,
            "groupBy": null, "columns": [], "cardProperties": [], "collapsedGroups": [],
            "zoom": 4
        }], "automations": []},
        "icon": "🧭"
    });
    let doc: Doc = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(doc.kind, DocKind::Other("whiteboard".into()));
    assert!(matches!(doc.author, Some(Person::Other(_))));
    let values = doc.values.as_ref().unwrap();
    assert!(matches!(values["a"], Value::Other(_)));
    assert!(matches!(values["b"], Value::Other(_)));
    assert!(matches!(&values["c"], Value::People(p) if matches!(p[1], Person::Other(_))));
    assert_eq!(values["d"], Value::Number(1.5));
    assert_eq!(values["e"], Value::Null);
    let fields = doc.database.as_ref().unwrap();
    assert_eq!(
        fields.properties[0].kind,
        PropertyType::Other("emoji".into())
    );
    assert_eq!(fields.views[0].kind, ViewType::Other("map".into()));
    assert_eq!(serde_json::to_value(&doc).unwrap(), json);
}

#[test]
fn created_by_and_edited_by_come_from_the_doc_and_round_trip() {
    let db_json = json!({
        "id": "5e0c9a1d-0000-4000-8000-000000000002", "kind": "database", "title": "Reading list",
        "order": "a0", "folderId": null, "parentDocId": null, "parentObjectId": null,
        "cover": null, "favorite": false, "agents": [], "author": {"kind": "owner"},
        "updatedBy": {"kind": "owner"}, "created": 1_793_000_000_000_i64,
        "updated": 1_793_000_000_000_i64, "rev": 1, "seq": 1, "deleted": false,
        "database": {"properties": [
            {"id": "name", "name": "Name", "type": "title", "config": {}, "order": "a0"},
            {"id": "by", "name": "Added by", "type": "createdBy", "config": {}, "order": "a1"},
            {"id": "editor", "name": "Edited by", "type": "editedBy", "config": {}, "order": "a2"}
        ], "views": []}
    });
    let db: Doc = serde_json::from_value(db_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&db).unwrap(), db_json);
    let fields = db.database.as_ref().unwrap();
    let (by, editor) = (
        fields.property("by").unwrap(),
        fields.property("editor").unwrap(),
    );
    assert_eq!(by.kind, PropertyType::CreatedBy);
    assert_eq!(editor.kind, PropertyType::EditedBy);
    for p in [by, editor] {
        assert!(p.kind.is_derived() && p.kind.can_group());
        assert!(!p.kind.is_date());
        assert_eq!(p.config, PropertyConfig::default());
    }
    assert_eq!(PropertyType::parse("createdBy").as_str(), "createdBy");
    assert_eq!(PropertyType::parse("editedBy").as_str(), "editedBy");
    assert!(Limits::default().check_doc(&db, None).is_empty());

    // The values are the doc's author and last editor, never anything in `values`.
    let rows_json = json!([
        {"id": "r1", "kind": "page", "title": "Tidewater Almanac", "order": "a0",
         "parentDocId": db.id, "author": {"kind": "owner"},
         "updatedBy": {"kind": "agent", "agentId": "inbox-triage"}, "values": {}},
        {"id": "r2", "kind": "page", "title": "Notes on Lanterns", "order": "a1",
         "parentDocId": db.id, "author": {"kind": "agent", "agentId": "career-planning"},
         "values": {}},
        {"id": "r3", "kind": "page", "title": "Untold Harbors", "order": "a2",
         "parentDocId": db.id, "values": {}}
    ]);
    let rows: Vec<Doc> = serde_json::from_value(rows_json).unwrap();
    assert_eq!(rows[0].created_by(), Some(&Person::Owner));
    assert_eq!(rows[0].edited_by(), Some(&Person::agent("inbox-triage")));
    // An older doc without updatedBy: its editor is its author.
    assert_eq!(rows[1].edited_by(), Some(&Person::agent("career-planning")));
    assert_eq!(rows[1].edited_by(), rows[1].created_by());
    assert_eq!((rows[2].created_by(), rows[2].edited_by()), (None, None));
    for r in &rows {
        assert!(Limits::default().check_row(fields, r).is_empty());
        assert_eq!(serde_json::to_value(r).unwrap()["values"], json!({}));
    }
}

#[test]
fn a_minimal_doc_reads_with_defaults() {
    let doc: Doc = serde_json::from_value(json!({"id": "x", "kind": "page"})).unwrap();
    assert_eq!(doc.title, "");
    let untitled: Doc =
        serde_json::from_value(json!({"id": "x", "kind": "page", "title": null, "order": null}))
            .unwrap();
    assert_eq!((untitled.title.as_str(), untitled.order.as_str()), ("", ""));
    assert!(!doc.deleted && doc.values.is_none() && doc.author.is_none());
}

#[test]
fn numbers_keep_their_written_form() {
    for (text, value) in [
        ("3", Value::Number(3.0)),
        ("-2.5", Value::Number(-2.5)),
        ("1e21", Value::Number(1e21)),
    ] {
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v, value);
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::from_str::<Json>(text).unwrap()
        );
    }
    // NaN can't be written, so it's no value.
    assert_eq!(Value::number(f64::NAN), Value::Null);
    let col: Column = serde_json::from_value(
        json!({"propertyId": "p", "width": 180.5, "hidden": false, "order": "a0"}),
    )
    .unwrap();
    assert_eq!(col.width, Some(180.5));
}

#[test]
fn edits_are_tagged_json() {
    let edit = Edit::RowValueChanged {
        row_id: "r".into(),
        property_id: "p".into(),
        value: None,
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        json!({"type": "rowValueChanged", "rowId": "r", "propertyId": "p", "value": null})
    );
    assert_eq!(edit.row_ids(), ["r"]);
    let open = TableEvent::OpenRow { row_id: "r".into() };
    assert_eq!(
        serde_json::to_value(&open).unwrap(),
        json!({"type": "openRow", "rowId": "r"})
    );
}

#[test]
fn new_ids_are_lowercase_uuids() {
    let id = new_id();
    assert_eq!(id.len(), 36);
    assert_eq!(id, id.to_lowercase());
    assert_ne!(id, new_id());
}

// The limits.

fn db_with(properties: Vec<Property>) -> DatabaseFields {
    DatabaseFields {
        properties,
        ..DatabaseFields::default()
    }
}

fn kinds(problems: &[Problem]) -> Vec<&ProblemKind> {
    problems.iter().map(|p| &p.kind).collect()
}

fn title() -> Property {
    Property::new("title", "Name", PropertyType::Title, "a0")
}

fn select(n: usize) -> Property {
    Property::new("s", "Stage", PropertyType::Select, "a1").with_config(PropertyConfig {
        options: Some(
            (0..n)
                .map(|i| SelectOption {
                    id: format!("o{i}"),
                    name: format!("Option {i}"),
                    color: Color::Default,
                })
                .collect(),
        ),
        ..PropertyConfig::default()
    })
}

#[test]
fn too_many_properties_options_views_and_sorts_are_reported() {
    let limits = Limits::default();
    let mut props = vec![title()];
    props.extend((0..200).map(|i| Property::new(format!("p{i}"), "x", PropertyType::Text, "a1")));
    let problems = limits.check_database(&db_with(props));
    assert_eq!(
        kinds(&problems),
        [&ProblemKind::TooMany {
            limit: 200,
            found: 201
        }]
    );
    assert_eq!(problems[0].at, "properties");

    let problems = limits.check_database(&db_with(vec![title(), select(201)]));
    assert_eq!(
        kinds(&problems),
        [&ProblemKind::TooMany {
            limit: 200,
            found: 201
        }]
    );
    assert_eq!(problems[0].at, "properties[1].config.options");

    let mut db = db_with(vec![title()]);
    db.views = (0..51)
        .map(|i| View::new(format!("v{i}"), "View", ViewType::Table, "a0"))
        .collect();
    db.views[0].sort = (0..11).map(|_| Sort::ascending("title")).collect();
    let problems = limits.check_database(&db);
    assert_eq!(
        kinds(&problems),
        [
            &ProblemKind::TooMany {
                limit: 50,
                found: 51
            },
            &ProblemKind::TooMany {
                limit: 10,
                found: 11
            }
        ]
    );
    assert_eq!(problems[1].at, "views[0].sort");
}

#[test]
fn malformed_databases_are_reported() {
    let limits = Limits::default();
    let problems = limits.check_database(&db_with(vec![select(2)]));
    assert_eq!(kinds(&problems), [&ProblemKind::NoTitle]);
    let mut two = title();
    two.id = "title2".into();
    let mut dup = select(2);
    dup.id = "title".into();
    let problems = limits.check_database(&db_with(vec![title(), two, dup]));
    assert_eq!(
        kinds(&problems),
        [
            &ProblemKind::Duplicate("title".into()),
            &ProblemKind::SeveralTitles
        ]
    );
    let mut long = select(1);
    long.name = "n".repeat(201);
    long.config.options.as_mut().unwrap()[0].name = "é".repeat(101);
    let problems = limits.check_database(&db_with(vec![title(), long]));
    assert_eq!(
        kinds(&problems),
        [
            &ProblemKind::TooLong {
                limit: 200,
                found: 201
            },
            &ProblemKind::TooLong {
                limit: 100,
                found: 101
            }
        ]
    );
}

#[test]
fn filters_too_deep_or_too_big_are_reported() {
    let limits = Limits::default();
    let leaf = || Filter::condition("title", FilterOp::IsNotEmpty, Json::Null);
    let mut deep = leaf();
    for _ in 0..4 {
        deep = Filter::And { and: vec![deep] };
    }
    assert_eq!(deep.depth(), 5);
    assert_eq!(
        kinds(&limits.check_filter(&deep, "f")),
        [&ProblemKind::TooDeep { limit: 4, found: 5 }]
    );
    let wide = Filter::Or {
        or: (0..100).map(|_| leaf()).collect(),
    };
    assert_eq!(
        kinds(&limits.check_filter(&wide, "f")),
        [&ProblemKind::TooMany {
            limit: 100,
            found: 101
        }]
    );

    // A tree far past the limits, built in code: measured without recursion, counted only so far.
    let mut huge = leaf();
    for _ in 0..100_000 {
        huge = Filter::And { and: vec![huge] };
    }
    let problems = limits.check_filter(&huge, "f");
    assert_eq!(problems.len(), 2);
    // Dropping a tree this deep would recurse; take it apart by hand.
    let mut stack = vec![huge];
    while let Some(f) = stack.pop() {
        if let Filter::And { and } = f {
            stack.extend(and);
        }
    }

    // From JSON, serde_json's own nesting limit stops a deep tree before it's built.
    let mut text = String::from(r#"{"propertyId":"t","op":"isEmpty"}"#);
    for _ in 0..200 {
        text = format!(r#"{{"and":[{text}]}}"#);
    }
    assert!(serde_json::from_str::<Filter>(&text).is_err());
}

#[test]
fn values_over_the_limits_or_of_the_wrong_shape_are_reported() {
    let limits = Limits::default();
    let text = Property::new("t", "Notes", PropertyType::Text, "a1");
    let multi = Property {
        kind: PropertyType::MultiSelect,
        ..select(150)
    };
    let rel =
        Property::new("r", "Company", PropertyType::Relation, "a2").with_config(PropertyConfig {
            database_id: Some("db".into()),
            ..PropertyConfig::default()
        });
    let person =
        Property::new("o", "Owner", PropertyType::Person, "a3").with_config(PropertyConfig {
            kinds: Some(vec![PersonKind::Agent]),
            ..PropertyConfig::default()
        });
    let num = Property::new("n", "Size", PropertyType::Number, "a4");
    let created = Property::new("c", "Added", PropertyType::CreatedTime, "a5");
    let by = Property::new("b", "Added by", PropertyType::CreatedBy, "a7");
    let editor = Property::new("e", "Edited by", PropertyType::EditedBy, "a8");

    let k = |p: &Property, v: Value| {
        limits
            .check_value(p, &v)
            .into_iter()
            .map(|p| p.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        k(&text, Value::text("x".repeat(10_001))),
        [ProblemKind::TooLong {
            limit: 10_000,
            found: 10_001
        }]
    );
    assert_eq!(k(&text, Value::text("é".repeat(10_000))), []);
    assert_eq!(
        k(&multi, Value::ids((0..101).map(|i| format!("o{i}")))),
        [ProblemKind::TooMany {
            limit: 100,
            found: 101
        }]
    );
    assert_eq!(
        k(&multi, Value::ids(["o1", "nope"])),
        [ProblemKind::UnknownOption("nope".into())]
    );
    assert_eq!(
        k(&multi, Value::ids(["o1", "o1"])),
        [ProblemKind::Duplicate("o1".into())]
    );
    assert_eq!(
        k(&select(3), Value::text("o9")),
        [ProblemKind::UnknownOption("o9".into())]
    );
    assert_eq!(
        k(&rel, Value::ids((0..2_001).map(|i| format!("r{i}")))),
        [ProblemKind::TooMany {
            limit: 2_000,
            found: 2_001
        }]
    );
    assert_eq!(
        k(&person, vec![Person::Owner].into()),
        [ProblemKind::KindNotAllowed]
    );
    assert_eq!(
        k(
            &person,
            Value::People((0..51).map(|i| Person::agent(format!("a{i}"))).collect())
        ),
        [ProblemKind::TooMany {
            limit: 50,
            found: 51
        }]
    );
    assert_eq!(
        k(&num, Value::text("12")),
        [ProblemKind::WrongShape {
            expected: PropertyType::Number
        }]
    );
    assert_eq!(k(&num, Value::Null), []);
    assert_eq!(k(&created, Value::Number(1.0)), [ProblemKind::Derived]);
    assert_eq!(k(&by, vec![Person::Owner].into()), [ProblemKind::Derived]);
    assert_eq!(k(&editor, Value::text("owner")), [ProblemKind::Derived]);
    assert_eq!(k(&by, Value::Null), []);
    let backwards = Value::Date(DateValue {
        start: 10,
        end: Some(5),
        include_time: false,
    });
    assert_eq!(
        k(
            &Property::new("d", "When", PropertyType::Date, "a6"),
            backwards
        ),
        [ProblemKind::EndsBeforeStart]
    );

    // A row's title, and where a value's problem is.
    let db = db_with(vec![title(), num.clone()]);
    let mut r = Doc::row("r1", "db", "t".repeat(2_001), "a0", Values::new());
    r.values
        .as_mut()
        .unwrap()
        .insert("n".into(), Value::text("x"));
    let problems = limits.check_row(&db, &r);
    assert_eq!(
        problems.iter().map(|p| p.at.as_str()).collect::<Vec<_>>(),
        ["title", "values.n"]
    );
    assert_eq!(
        problems[0].to_string(),
        "title: 2001 characters is more than 2000"
    );
}

#[test]
fn clamping_cuts_values_to_the_limits() {
    let limits = Limits::default();
    let url = Property::new("u", "Site", PropertyType::Url, "a1");
    let Value::Text(s) = limits.clamp_value(&url, Value::text("ü".repeat(3_000))) else {
        panic!()
    };
    assert_eq!(s.chars().count(), 2_048);
    let multi = Property {
        kind: PropertyType::MultiSelect,
        ..select(3)
    };
    let clamped = limits.clamp_value(
        &multi,
        Value::ids((0..500).map(|i| format!("o{}", i % 120))),
    );
    assert_eq!(clamped.as_ids().unwrap().len(), 100);
    let person = Property::new("o", "Owner", PropertyType::Person, "a3");
    let many: Vec<Person> = (0..80)
        .map(|i| Person::agent(format!("a{}", i % 60)))
        .collect();
    let Value::People(p) = limits.clamp_value(&person, many.into()) else {
        panic!()
    };
    assert_eq!(p.len(), 50);
    assert_eq!(
        limits.clamp_title(&"日本".repeat(1_500)).chars().count(),
        2_000
    );
    assert_eq!(
        limits.clamp_value(&url, Value::Number(3.0)),
        Value::Number(3.0)
    );
}
