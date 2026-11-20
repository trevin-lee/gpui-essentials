//! The fractional-index vectors in tests/fixtures/order-vectors.json, which other
//! implementations can test against too: every `between`, `n_between` and `spread` case gives the expected keys
//! or the expected error.

use gpui_tables::order;
use serde_json::Value as Json;

fn vectors() -> Json {
    let path = format!(
        "{}/tests/fixtures/order-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn key(j: &Json) -> Option<&str> {
    j.as_str()
}

/// What a case expects: keys, or an error code.
fn expected(case: &Json) -> Result<Json, String> {
    match case.get("error") {
        Some(e) => Err(e.as_str().unwrap().to_owned()),
        None => Ok(case["expected"].clone()),
    }
}

#[test]
fn the_alphabet_and_cap_are_the_crates() {
    let v = vectors();
    assert_eq!(v["maxLength"], order::MAX_LEN);
    assert_eq!(
        v["alphabet"],
        "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
    );
}

#[test]
fn between_vectors() {
    let v = vectors();
    let cases = v["between"].as_array().unwrap();
    assert!(cases.len() >= 40);
    for case in cases {
        let got = order::between(key(&case["a"]), key(&case["b"]))
            .map(Json::String)
            .map_err(|e| e.code().to_owned());
        assert_eq!(got, expected(case), "{}", case["note"]);
        if let Ok(k) = &got {
            assert!(order::is_valid(k.as_str().unwrap()));
        }
    }
}

#[test]
fn n_between_vectors() {
    let v = vectors();
    for case in v["nBetween"].as_array().unwrap() {
        let n = case["n"].as_u64().unwrap() as usize;
        let got = order::n_between(key(&case["a"]), key(&case["b"]), n)
            .map(Json::from)
            .map_err(|e| e.code().to_owned());
        assert_eq!(got, expected(case), "{}", case["note"]);
    }
}

#[test]
fn spread_vectors() {
    let v = vectors();
    for case in v["spread"].as_array().unwrap() {
        let n = case["n"].as_u64().unwrap() as usize;
        assert_eq!(
            Json::from(order::spread(n)),
            case["expected"],
            "spread({n})"
        );
    }
}
