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
fn deployment_events_help_shows_json_flag() {
    stacker_cmd()
        .args(["deployment", "events", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").and(predicate::str::contains("--deployment")));
}

#[test]
fn deployment_events_json_fetches_structured_feed() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_events_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let events_mock = server
        .mock("GET", "/api/v1/deployments/deployment_events_online/events")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment events fetched",
                "item": {
                    "schemaVersion": "v1alpha1",
                    "deploymentHash": "deployment_events_online",
                    "events": [
                        {
                            "sequence": 1,
                            "kind": "command_queued",
                            "classification": "info",
                            "occurredAt": "2026-05-17T08:00:00Z",
                            "summary": "deploy_app queued",
                            "commandId": "cmd-1",
                            "commandType": "deploy_app",
                            "status": "queued"
                        },
                        {
                            "sequence": 2,
                            "kind": "command_failed",
                            "classification": "failure",
                            "occurredAt": "2026-05-17T08:03:00Z",
                            "summary": "Compose path could not be resolved",
                            "commandId": "cmd-1",
                            "commandType": "deploy_app",
                            "status": "failed",
                            "retryable": false,
                            "remediationClass": "configuration"
                        }
                    ]
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deployment", "events", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains(
                    "\"deploymentHash\": \"deployment_events_online\"",
                ))
                .and(predicate::str::contains("\"kind\": \"command_failed\""))
                .and(predicate::str::contains(
                    "\"remediationClass\": \"configuration\"",
                )),
        );

    events_mock.assert();
}
