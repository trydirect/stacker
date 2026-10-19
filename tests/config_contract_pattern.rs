//! config_contract limits the platform enforces on Quick Deploy values:
//! `pattern` and `max_length`, and `length` meaning "exactly" everywhere.
//!
//! prod 2026-10-19: a buyer typed 4 characters into SUPABASE_SECRET_KEY /
//! OPENAI_API_KEY and nothing could refuse it - the contract had no way to
//! say what a key looks like. And the alphanumeric generator read min_length
//! as the exact size while ignoring length.
//! Scenarios: config shared-fixtures/features/quick-deploy-env-field-rules.feature

use serde_json::json;
use stacker::cli::config_parser::FieldPolicy;
use stacker::console::commands::cli::init::generator_shell_expression;

fn policy(value: serde_json::Value) -> Result<FieldPolicy, serde_json::Error> {
    serde_json::from_value(value)
}

#[test]
fn pattern_and_max_length_are_kept() {
    let p = policy(json!({
        "mutability": "provided", "required": false,
        "pattern": "^sk-[A-Za-z0-9_-]+$", "min_length": 40, "max_length": 256
    }))
    .expect("a provided field with pattern and max_length parses");
    assert_eq!(p.pattern.as_deref(), Some("^sk-[A-Za-z0-9_-]+$"));
    assert_eq!(p.max_length, Some(256));
    assert_eq!(p.min_length, Some(40));
}

#[test]
fn a_contract_nothing_could_satisfy_is_refused() {
    for (name, value) in [
        ("pattern that does not compile", json!({"mutability": "provided", "pattern": "^(a"})),
        (
            "min_length over length",
            json!({"mutability": "generated", "type": "alphanumeric", "length": 16, "min_length": 32}),
        ),
        ("min_length over max_length", json!({"mutability": "provided", "min_length": 300, "max_length": 256})),
        ("length over max_length", json!({"mutability": "provided", "length": 300, "max_length": 256})),
    ] {
        assert!(policy(value).is_err(), "{name} must be refused");
    }
}

#[test]
fn an_alphanumeric_length_is_generated_exactly() {
    let p = policy(json!({"mutability": "generated", "type": "alphanumeric", "length": 16}))
        .expect("parses");
    let expr = generator_shell_expression(&p).expect("alphanumeric is generated");
    assert!(expr.ends_with("head -c 16"), "{expr}");
}

#[test]
fn an_alphanumeric_min_length_alone_still_gives_that_many() {
    let p = policy(json!({"mutability": "generated", "type": "alphanumeric", "min_length": 32}))
        .expect("parses");
    let expr = generator_shell_expression(&p).expect("alphanumeric is generated");
    assert!(expr.ends_with("head -c 32"), "{expr}");
}
