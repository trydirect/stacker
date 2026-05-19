use assert_cmd::Command;
use chrono::{Duration, Utc};
use mockito::{Matcher, Server};
use predicates::prelude::*;
use serde_json::json;
use stacker::cli::credentials::StoredCredentials;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn write_stacker_config(dir: &TempDir, deployment_hash: &str) {
    let config = format!(
        r#"
name: local-name
project:
  identity: remote-project
app:
  type: static
  path: "."
deploy:
  target: cloud
  deployment_hash: {deployment_hash}
"#
    );
    fs::write(dir.path().join("stacker.yml"), config).expect("write stacker.yml");
    fs::write(dir.path().join("index.html"), "<h1>Hello</h1>").expect("write index.html");
}

fn write_credentials(config_home: &TempDir, server_url: &str) {
    let creds = StoredCredentials {
        access_token: "tok".to_string(),
        refresh_token: Some("rtok".to_string()),
        token_type: "Bearer".to_string(),
        expires_at: Utc::now() + Duration::hours(1),
        email: Some("user@example.com".to_string()),
        server_url: Some(server_url.to_string()),
        org: None,
        domain: None,
    };

    let cred_dir = config_home.path().join("stacker");
    fs::create_dir_all(&cred_dir).expect("create credentials dir");
    fs::write(
        cred_dir.join("credentials.json"),
        serde_json::to_vec(&creds).expect("serialize credentials"),
    )
    .expect("write credentials");
}

#[test]
fn deployment_rollback_plan_outputs_preview_json() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let mock = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("operation".into(), "rollback_deploy".into()),
            Matcher::UrlEncoded("target".into(), "cloud".into()),
            Matcher::UrlEncoded("rollbackTarget".into(), "previous".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment plan fetched",
                "item": serde_json::from_str::<serde_json::Value>(include_str!("contracts/stacker-deploy-plan.v1alpha1.rollback-previous.json")).unwrap()
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deployment", "rollback", "--to", "previous", "--plan"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"operation\": \"rollback_deploy\"")
                .and(predicate::str::contains("\"resolvedVersion\": \"1.1.0\"")),
        );

    mock.assert();
}

#[test]
fn deployment_rollback_apply_validates_plan_and_posts_version() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let plan = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("operation".into(), "rollback_deploy".into()),
            Matcher::UrlEncoded("target".into(), "cloud".into()),
            Matcher::UrlEncoded("rollbackTarget".into(), "previous".into()),
            Matcher::UrlEncoded("expectedFingerprint".into(), "plan-rollback-previous".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment plan fetched",
                "item": serde_json::from_str::<serde_json::Value>(include_str!("contracts/stacker-deploy-plan.v1alpha1.rollback-previous.json")).unwrap()
            })
            .to_string(),
        )
        .create();

    let list_projects = server
        .mock("GET", "/project")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "message": "OK",
                "list": [
                    {
                        "id": 42,
                        "name": "remote-project",
                        "user_id": "user-1",
                        "metadata": {},
                        "created_at": "2026-04-13T00:00:00Z",
                        "updated_at": "2026-04-13T00:00:00Z"
                    }
                ]
            })
            .to_string(),
        )
        .create();

    let rollback = server
        .mock("POST", "/project/42/rollback")
        .match_header("authorization", "Bearer tok")
        .match_header(
            "content-type",
            Matcher::Regex("application/json.*".to_string()),
        )
        .match_body(Matcher::Json(json!({ "version": "1.1.0" })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "id": 42,
                "_status": "ok",
                "msg": "Success",
                "meta": { "deployment_id": 99 }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args([
            "deployment",
            "rollback",
            "--to",
            "previous",
            "--apply-plan",
            "plan-rollback-previous",
            "--confirm",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("version '1.1.0'"));

    plan.assert();
    list_projects.assert();
    rollback.assert();
}

#[test]
fn deployment_rollback_plan_surfaces_typed_unsupported_error() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let mock = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("operation".into(), "rollback_deploy".into()),
            Matcher::UrlEncoded("target".into(), "cloud".into()),
            Matcher::UrlEncoded("rollbackTarget".into(), "previous".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "schemaVersion": "v1alpha1",
                "code": "rollback_target_unavailable",
                "message": "Rollback is only available for marketplace deployments with an older template version",
                "retryable": false,
                "remediationClass": "state",
                "context": {
                    "rollbackTarget": "previous"
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deployment", "rollback", "--to", "previous", "--plan"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("rollback_target_unavailable")
                .and(predicate::str::contains("rollbackTarget")),
        );

    mock.assert();
}
