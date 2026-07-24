use cucumber::then;

use super::StepWorld;

#[then(regex = r#"^the response content-type should contain "([^"]+)"$"#)]
async fn then_content_type_contains(world: &mut StepWorld, expected: String) {
    let content_type = world
        .response_headers
        .as_ref()
        .and_then(|h| h.get("content-type"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains(&expected),
        "Expected content-type to contain '{}', got '{}'",
        expected,
        content_type
    );
}
