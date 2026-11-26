//! BDD steps for the GitHub repo-connect REST API (WS-G).

use super::StepWorld;
use cucumber::{given, then, when};
use serde_json::json;

#[when(regex = r#"^I connect a github repo "([^"]+)" on branch "([^"]+)"$"#)]
async fn connect_repo(world: &mut StepWorld, repo: String, branch: String) {
    let project_id = world
        .stored_ids
        .get("project_id")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let body = json!({
        "projectId": project_id,
        "installationId": 1,
        "repo": repo,
        "branch": branch,
        "watchPaths": ["src/**"],
        "autodeploy": true,
    });
    world.post_json("/api/v1/github/repos", &body).await;
    world.store_id_from_response("repo_id", "/item/id");
}

#[given(regex = r#"^I have connected a github repo "([^"]+)" on branch "([^"]+)"$"#)]
async fn given_connected_repo(world: &mut StepWorld, repo: String, branch: String) {
    connect_repo(world, repo, branch).await;
}

#[when("I list github repos")]
async fn list_repos(world: &mut StepWorld) {
    world.get("/api/v1/github/repos").await;
}

#[when("I delete the stored github repo")]
async fn delete_stored_repo(world: &mut StepWorld) {
    let id = world.stored_ids.get("repo_id").cloned().unwrap_or_default();
    world.delete(&format!("/api/v1/github/repos/{}", id)).await;
}

#[when("I fetch the stored github repo")]
async fn fetch_stored_repo(world: &mut StepWorld) {
    let id = world.stored_ids.get("repo_id").cloned().unwrap_or_default();
    world.get(&format!("/api/v1/github/repos/{}", id)).await;
}

#[then(regex = r#"^the repo list contains "([^"]+)"$"#)]
fn repo_list_contains(world: &mut StepWorld, repo: String) {
    let list = world
        .response_json
        .as_ref()
        .and_then(|v| v.pointer("/list"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        list.iter()
            .any(|r| r.get("repo").and_then(|n| n.as_str()) == Some(repo.as_str())),
        "repo {:?} not found in list: {:?}",
        repo,
        list
    );
}
