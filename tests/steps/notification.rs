//! BDD steps for the notification channel REST API (WS-N).

use super::StepWorld;
use cucumber::{given, then, when};
use serde_json::json;

#[when(regex = r#"^I create a notification channel "([^"]+)" of type "([^"]+)"$"#)]
async fn create_channel(world: &mut StepWorld, name: String, channel_type: String) {
    let project_id = world
        .stored_ids
        .get("project_id")
        .and_then(|v| v.parse::<i32>().ok());
    let body = json!({
        "name": name,
        "projectId": project_id,
        "channelType": channel_type,
        "secret": { "url": "https://hooks.example/x" },
        "events": ["backup.failed"],
    });
    world
        .post_json("/api/v1/notifications/channels", &body)
        .await;
    world.store_id_from_response("channel_id", "/item/id");
}

#[given(regex = r#"^I have created a notification channel "([^"]+)" of type "([^"]+)"$"#)]
async fn given_created_channel(world: &mut StepWorld, name: String, channel_type: String) {
    create_channel(world, name, channel_type).await;
}

#[when("I list notification channels")]
async fn list_channels(world: &mut StepWorld) {
    world.get("/api/v1/notifications/channels").await;
}

#[when("I send a test message to the stored notification channel")]
async fn test_stored_channel(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("channel_id")
        .cloned()
        .unwrap_or_default();
    world
        .post_json(
            &format!("/api/v1/notifications/channels/{}/test", id),
            &json!({}),
        )
        .await;
}

#[when("I delete the stored notification channel")]
async fn delete_stored_channel(world: &mut StepWorld) {
    let id = world
        .stored_ids
        .get("channel_id")
        .cloned()
        .unwrap_or_default();
    world
        .delete(&format!("/api/v1/notifications/channels/{}", id))
        .await;
}

#[then(regex = r#"^the channel list contains "([^"]+)"$"#)]
fn channel_list_contains(world: &mut StepWorld, name: String) {
    let list = world
        .response_json
        .as_ref()
        .and_then(|v| v.pointer("/list"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        list.iter()
            .any(|c| c.get("name").and_then(|n| n.as_str()) == Some(name.as_str())),
        "channel {:?} not found in list: {:?}",
        name,
        list
    );
}
