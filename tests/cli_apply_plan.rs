use assert_cmd::Command;
use chrono::{Duration, Utc};
use mockito::Server;
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
  environment: prod
environments:
  prod:
    compose_file: docker/prod/compose.yml
    env_file: docker/prod/.env
"#
    );
    fs::create_dir_all(dir.path().join("docker/prod")).expect("create docker/prod");
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
fn deploy_apply_plan_rejects_stale_fingerprint() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let stale = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
            mockito::Matcher::UrlEncoded("expectedFingerprint".into(), "stale-fingerprint".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(409)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "schemaVersion": "v1alpha1",
                "code": "plan_stale",
                "message": "Plan input is stale; regenerate the plan before apply",
                "retryable": false,
                "remediationClass": "state",
                "context": {
                    "expectedFingerprint": "stale-fingerprint",
                    "actualFingerprint": "fresh-fingerprint"
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deploy", "--apply-plan", "stale-fingerprint"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("plan_stale")
                .and(predicate::str::contains("expectedFingerprint"))
                .and(predicate::str::contains("fresh-fingerprint")),
        );

    stale.assert();
}

#[test]
fn deploy_apply_plan_is_idempotent_when_plan_is_already_satisfied() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let body = json!({
        "_status": "OK",
        "msg": "Deployment plan fetched",
        "item": {
            "schemaVersion": "v1alpha1",
            "deploymentHash": "deployment_state_online",
            "operation": "deploy",
            "target": "cloud",
            "fingerprint": "plan-no-changes",
            "scope": {
                "mode": "deployment",
                "selectedApps": ["device-api", "upload"]
            },
            "hasChanges": false,
            "actions": [],
            "reasoning": ["no drift detected for the selected scope"]
        }
    })
    .to_string();

    let mock1 = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
            mockito::Matcher::UrlEncoded("expectedFingerprint".into(), "plan-no-changes".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body.clone())
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deploy", "--apply-plan", "plan-no-changes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Plan already satisfied"));

    let mock2 = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
            mockito::Matcher::UrlEncoded("expectedFingerprint".into(), "plan-no-changes".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deploy", "--apply-plan", "plan-no-changes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Plan already satisfied"));

    mock1.assert();
    mock2.assert();
}

#[test]
fn agent_deploy_app_apply_plan_validates_then_executes() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let plan = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy_app".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
            mockito::Matcher::UrlEncoded("appCode".into(), "upload".into()),
            mockito::Matcher::UrlEncoded("expectedFingerprint".into(), "plan-apply-upload".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment plan fetched",
                "item": {
                    "schemaVersion": "v1alpha1",
                    "deploymentHash": "deployment_state_online",
                    "operation": "deploy_app",
                    "target": "cloud",
                    "fingerprint": "plan-apply-upload",
                    "scope": {
                        "mode": "app",
                        "appCode": "upload",
                        "selectedApps": ["upload"]
                    },
                    "hasChanges": true,
                    "actions": [
                        {
                            "kind": "redeploy_app",
                            "target": "app",
                            "appCode": "upload",
                            "reason": "explicit deploy-app plan targets a single app"
                        }
                    ],
                    "reasoning": ["deploy-app scope is restricted to the requested app"]
                }
            })
            .to_string(),
        )
        .create();

    let enqueue_check = server
        .mock("POST", "/api/v1/agent/commands/enqueue")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd-check",
                    "deployment_hash": "deployment_state_online",
                    "type": "check_connections",
                    "status": "pending",
                    "priority": "normal",
                    "parameters": {},
                    "result": null,
                    "error": null,
                    "created_at": "2026-05-17T12:00:00Z",
                    "updated_at": "2026-05-17T12:00:00Z"
                }
            })
            .to_string(),
        )
        .create();

    let status_check = server
        .mock("GET", "/api/v1/commands/deployment_state_online/cmd-check")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd-check",
                    "deployment_hash": "deployment_state_online",
                    "type": "check_connections",
                    "status": "completed",
                    "priority": "normal",
                    "parameters": {},
                    "result": {
                        "active_connections": 0,
                        "ports": []
                    },
                    "error": null,
                    "created_at": "2026-05-17T12:00:00Z",
                    "updated_at": "2026-05-17T12:00:01Z"
                }
            })
            .to_string(),
        )
        .create();

    let enqueue_deploy = server
        .mock("POST", "/api/v1/agent/commands/enqueue")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd-1",
                    "deployment_hash": "deployment_state_online",
                    "type": "deploy_app",
                    "status": "pending",
                    "priority": "normal",
                    "parameters": {},
                    "result": null,
                    "error": null,
                    "created_at": "2026-05-17T12:00:00Z",
                    "updated_at": "2026-05-17T12:00:00Z"
                }
            })
            .to_string(),
        )
        .create();

    let status_deploy = server
        .mock("GET", "/api/v1/commands/deployment_state_online/cmd-1")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd-1",
                    "deployment_hash": "deployment_state_online",
                    "type": "deploy_app",
                    "status": "completed",
                    "priority": "normal",
                    "parameters": {},
                    "result": {
                        "status": "ok",
                        "message": "upload redeployed"
                    },
                    "error": null,
                    "created_at": "2026-05-17T12:00:00Z",
                    "updated_at": "2026-05-17T12:00:03Z"
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args([
            "agent",
            "deploy-app",
            "upload",
            "--apply-plan",
            "plan-apply-upload",
            "--deployment",
            "deployment_state_online",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("cmd-1").and(predicate::str::contains("deploy_app")));

    plan.assert();
    enqueue_check.assert();
    status_check.assert();
    enqueue_deploy.assert();
    status_deploy.assert();
}
