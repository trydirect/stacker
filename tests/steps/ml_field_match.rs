use cucumber::{then, when};
use serde_json::json;

use super::StepWorld;

// ─── Field Matching Steps ────────────────────────────────────────

#[when(
    regex = r#"^I request field matching with source fields "([^"]*)" and target fields "([^"]*)"$"#
)]
async fn when_request_field_matching(
    world: &mut StepWorld,
    source_fields_csv: String,
    target_fields_csv: String,
) {
    let source_fields: Vec<String> = source_fields_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let target_fields: Vec<String> = target_fields_csv
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    world
        .post_json(
            "/api/v1/pipes/field-match",
            &json!({
                "source_fields": source_fields,
                "target_fields": target_fields,
            }),
        )
        .await;
}

#[then(regex = r#"^the field match for "([^"]+)" should map to "([^"]+)"$"#)]
async fn then_field_match_maps(world: &mut StepWorld, target_field: String, source_field: String) {
    let json = world.response_json.as_ref().expect("no response JSON");
    let item = &json["item"];
    let mapping = &item["mapping"];

    let mapped = mapping
        .get(&target_field)
        .unwrap_or_else(|| panic!("no mapping for target field '{}'", target_field));
    let mapped_str = mapped.as_str().expect("mapping value not a string");
    let actual_source = mapped_str.trim_start_matches("$.");

    assert_eq!(
        actual_source, source_field,
        "expected '{}' to map to '{}', got '{}'",
        target_field, source_field, actual_source
    );
}

#[then(regex = r#"^the field match confidence for "([^"]+)" should be at least ([0-9.]+)$"#)]
async fn then_field_match_confidence(
    world: &mut StepWorld,
    field_name: String,
    min_confidence: f32,
) {
    let json = world.response_json.as_ref().expect("no response JSON");
    let item = &json["item"];
    let confidence = &item["confidence"];

    let score = confidence
        .get(&field_name)
        .unwrap_or_else(|| panic!("no confidence score for '{}'", field_name))
        .as_f64()
        .expect("confidence not a number") as f32;

    assert!(
        score >= min_confidence,
        "confidence for '{}' is {}, expected at least {}",
        field_name,
        score,
        min_confidence
    );
}

#[then(regex = r#"^the field match result should have unmatched target "([^"]+)"$"#)]
async fn then_unmatched_target(world: &mut StepWorld, field_name: String) {
    let json = world.response_json.as_ref().expect("no response JSON");
    let item = &json["item"];
    let unmatched = item["unmatched_target"]
        .as_array()
        .expect("unmatched_target not an array");

    let found = unmatched
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s == field_name));

    assert!(
        found,
        "'{}' not found in unmatched_target: {:?}",
        field_name, unmatched
    );
}

#[then("the field match mapping should be empty")]
async fn then_mapping_empty(world: &mut StepWorld) {
    let json = world.response_json.as_ref().expect("no response JSON");
    let item = &json["item"];
    let mapping = item["mapping"].as_object().expect("mapping not an object");

    assert!(
        mapping.is_empty(),
        "expected empty mapping, got {:?}",
        mapping
    );
}
