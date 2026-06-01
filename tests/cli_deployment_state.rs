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
fn deployment_state_help_shows_json_flag() {
    stacker_cmd()
        .args(["deployment", "state", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").and(predicate::str::contains("--deployment")));
}

#[test]
fn deployment_state_json_fetches_canonical_payload() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let state = json!({
        "schemaVersion": "v1alpha1",
        "project": {
            "id": 17,
            "identity": "remote-project",
            "name": "Remote Project"
        },
        "deployment": {
            "id": 31,
            "deploymentHash": "deployment_state_online",
            "status": "healthy",
            "runtime": "runc"
        },
        "agent": {
            "id": "agent-1",
            "status": "online",
            "version": "0.1.9",
            "lastHeartbeat": "2026-05-17T08:15:00Z",
            "capabilities": ["docker", "compose", "logs"],
            "features": {
                "compose": true,
                "kataRuntime": false,
                "backup": false,
                "pipes": false,
                "proxyCredentialsVault": false
            }
        },
        "runtime": {
            "composePath": "/home/trydirect/project/docker-compose.yml",
            "envPath": "/home/trydirect/project/.env"
        },
        "apps": [],
        "drift": {
            "hasDrift": false,
            "summary": "no drift detected"
        }
    });

    let state_mock = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/state")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment state fetched",
                "item": state
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deployment", "state", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains(
                    "\"deploymentHash\": \"deployment_state_online\"",
                ))
                .and(predicate::str::contains(
                    "\"composePath\": \"/home/trydirect/project/docker-compose.yml\"",
                )),
        );

    state_mock.assert();
}
