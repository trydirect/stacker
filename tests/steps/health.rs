use cucumber::{then, when};

use super::StepWorld;

#[when(regex = r#"^I send a GET request to "(.+)"$"#)]
async fn send_get_request(world: &mut StepWorld, path: String) {
    world.get(&path).await;
}

#[then(regex = r#"^the response status should be (\d+)$"#)]
async fn check_status(world: &mut StepWorld, expected: u16) {
    let actual = world.status_code.expect("No response received yet");
    assert_eq!(
        actual, expected,
        "Expected status {}, got {}. Body: {}",
        expected,
        actual,
        world.response_body.as_deref().unwrap_or("<none>")
    );
}

#[then(regex = r#"^the response status should be one of "(.+)"$"#)]
async fn check_status_one_of(world: &mut StepWorld, codes: String) {
    let actual = world.status_code.expect("No response received yet");
    let allowed: Vec<u16> = codes
        .split(',')
        .map(|s| s.trim().parse().expect("Invalid status code in list"))
        .collect();
    assert!(
        allowed.contains(&actual),
        "Expected status to be one of {:?}, got {}",
        allowed,
        actual
    );
}

#[then(regex = r#"^the response JSON should have key "(.+)"$"#)]
async fn check_json_has_key(world: &mut StepWorld, key: String) {
    let json = world
        .response_json
        .as_ref()
        .expect("No JSON response available");
    assert!(
        json.get(&key).is_some(),
        "Expected JSON to have key '{}', got: {}",
        key,
        json
    );
}

#[then(regex = r#"^the response JSON at "(.+)" should be "(.+)"$"#)]
async fn check_json_at_path(world: &mut StepWorld, path: String, expected: String) {
    let json = world
        .response_json
        .as_ref()
        .expect("No JSON response available");
    let value = json
        .pointer(&path)
        .unwrap_or_else(|| panic!("JSON path '{}' not found in: {}", path, json));
    let actual = match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    assert_eq!(actual, expected, "At JSON path '{}'", path);
}

#[then("the response body should be empty")]
async fn check_empty_body(world: &mut StepWorld) {
    let body = world.response_body.as_deref().unwrap_or("");
    assert!(body.is_empty(), "Expected empty body, got: {}", body);
}

#[then(regex = r#"^the response JSON at "(.+)" should not be empty$"#)]
async fn check_json_at_path_not_empty(world: &mut StepWorld, path: String) {
    let json = world
        .response_json
        .as_ref()
        .expect("No JSON response available");
    let value = json
        .pointer(&path)
        .unwrap_or_else(|| panic!("JSON path '{}' not found in: {}", path, json));
    match value {
        serde_json::Value::String(s) => {
            assert!(!s.is_empty(), "Expected non-empty string at '{}', got empty", path);
        }
        serde_json::Value::Null => {
            panic!("Expected non-empty value at '{}', got null", path);
        }
        _ => {} // non-null, non-string is fine
    }
}
