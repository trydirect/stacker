//! BDD steps for the backup REST API (WS-1).

use super::StepWorld;
use cucumber::{given, then, when};
use serde_json::json;

#[when(regex = r#"^I create a backup destination "([^"]+)" at "([^"]+)"$"#)]
async fn create_destination(world: &mut StepWorld, name: String, endpoint: String) {
    let body = json!({
        "name": name,
        "endpoint": endpoint,
        "vaultPath": format!("backup/{name}"),
    });
    world.post_json("/api/v1/backups/destinations", &body).await;
    world.store_id_from_response("destination_id", "/item/id");
}

#[given(regex = r#"^I have created a backup destination "([^"]+)" at "([^"]+)"$"#)]
async fn given_created_destination(world: &mut StepWorld, name: String, endpoint: String) {
    create_destination(world, name, endpoint).await;
}

#[when(regex = r#"^I create a backup job "([^"]+)" with source "([^"]+)" targeting "([^"]+)"$"#)]
async fn create_job(world: &mut StepWorld, name: String, source: String, target: String) {
    let project_id = world
        .stored_ids
        .get("project_id")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let destination_id = world
        .stored_ids
        .get("destination_id")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let body = json!({
        "projectId": project_id,
        "destinationId": destination_id,
        "name": name,
        "source": source,
        "target": target,
        "database": "shop",
        "schedule": "0 3 * * *",
    });
    world.post_json("/api/v1/backups/jobs", &body).await;
    world.store_id_from_response("job_id", "/item/id");
}

#[given(
    regex = r#"^I have created a backup job "([^"]+)" with source "([^"]+)" targeting "([^"]+)"$"#
)]
async fn given_created_job(world: &mut StepWorld, name: String, source: String, target: String) {
    create_job(world, name, source, target).await;
}

#[when("I list backup jobs")]
async fn list_jobs(world: &mut StepWorld) {
    world.get("/api/v1/backups/jobs").await;
}

#[when("I list backup destinations")]
async fn list_destinations(world: &mut StepWorld) {
    world.get("/api/v1/backups/destinations").await;
}

#[when("I fetch the stored backup job")]
async fn fetch_stored_job(world: &mut StepWorld) {
    let id = world.stored_ids.get("job_id").cloned().unwrap_or_default();
    world.get(&format!("/api/v1/backups/jobs/{}", id)).await;
}

#[when(regex = r#"^I delete the stored backup job$"#)]
async fn delete_stored_job(world: &mut StepWorld) {
    let id = world.stored_ids.get("job_id").cloned().unwrap_or_default();
    world.delete(&format!("/api/v1/backups/jobs/{}", id)).await;
}

#[then(regex = r#"^the response does not contain "([^"]+)"$"#)]
fn response_not_contains(world: &mut StepWorld, text: String) {
    let body = world.response_body.clone().unwrap_or_default();
    assert!(
        !body.contains(&text),
        "response unexpectedly contains {:?}: {}",
        text,
        body
    );
}

#[then(regex = r#"^the job list contains "([^"]+)"$"#)]
fn job_list_contains(world: &mut StepWorld, name: String) {
    let list = world
        .response_json
        .as_ref()
        .and_then(|v| v.pointer("/list"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        list.iter()
            .any(|j| j.get("name").and_then(|n| n.as_str()) == Some(name.as_str())),
        "job {:?} not found in list: {:?}",
        name,
        list
    );
}
