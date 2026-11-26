//! Regression for BUGS.md #61 and #60.
//!
//! #61 — a command to a deployment whose agent cannot pick it up must be
//! refused **immediately**, naming the true cause and the one command that
//! fixes it. Waiting ends the same way, only after the full poll deadline.
//! A slow but alive agent must never be refused early — a late answer is
//! still an answer.
//!
//! #60 — when the agent refuses a command, the warning must say so. "Agent
//! could not verify active connections" sent readers to look at the network
//! while the event log said `program 'check_connections' is not allowed`.

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

fn setup_project(dir: &TempDir) {
    fs::write(
        dir.path().join("stacker.yml"),
        "name: agent-liveness\nversion: \"1.0\"\napp:\n  type: static\n  path: \".\"\ndeploy:\n  target: local\n",
    )
    .expect("write stacker.yml");
    fs::write(dir.path().join("index.html"), "<h1>x</h1>").expect("write index.html");
}

fn snapshot_body(deployment_hash: &str, last_heartbeat: serde_json::Value) -> String {
    json!({
        "_status": "OK",
        "msg": "Snapshot fetched",
        "item": {
            "project_id": 42,
            "agent": {
                "id": 7,
                "version": "1.0.0",
                "status": "offline",
                "capabilities": null,
                "system_info": null,
                "last_heartbeat": last_heartbeat,
                "deployment_hash": deployment_hash,
            },
            "commands": [],
            "containers": [],
            "apps": [],
        }
    })
    .to_string()
}

fn deployments_list_body() -> String {
    json!({
        "_status": "OK",
        "list": [{
            "id": 1553,
            "project_id": 42,
            "deployment_hash": "deployment_live",
            "status": "completed",
            "status_message": null,
            "created_at": "2026-11-21T10:00:00Z",
            "updated_at": "2026-11-21T10:05:00Z",
        }]
    })
    .to_string()
}

fn command_body(command_id: &str, deployment_hash: &str, status: &str) -> String {
    json!({
        "_status": "OK",
        "item": {
            "command_id": command_id,
            "deployment_hash": deployment_hash,
            "type": "health",
            "status": status,
        }
    })
    .to_string()
}

/// BUGS.md #61: refused fast, with the cause and the fix — never queued into
/// a wall. On the old code this same run waited out `--timeout 5` and
/// reported "timed out (last status: queued)".
#[test]
fn a_command_to_an_offline_agent_is_refused_fast_with_the_fix() {
    let mut server = Server::new();
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    write_credentials(&config_home, &server.url());
    setup_project(&dir);

    let stale = "2026-11-19T18:32:00Z";
    let _snapshot = server
        .mock("GET", "/api/v1/agent/deployments/deployment_dead")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(snapshot_body("deployment_dead", json!(stale)))
        .create();
    let _list = server
        .mock("GET", "/api/v1/deployments")
        .match_query(Matcher::Any)
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(deployments_list_body())
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args([
            "agent",
            "exec",
            "health",
            "--timeout",
            "5",
            "--deployment",
            "deployment_dead",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("No agent is picking up commands")
                .and(predicate::str::contains("minutes ago"))
                .and(predicate::str::contains(
                    "stacker deployment use deployment_live",
                )),
        )
        .stderr(predicate::str::contains("timed out").not());
}

/// BUGS.md #60: a command the agent refused is reported as a refusal, with
/// the agent's own words — not as silence or a network mystery.
#[test]
fn a_refused_command_is_reported_as_a_refusal() {
    let mut server = Server::new();
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    write_credentials(&config_home, &server.url());
    setup_project(&dir);

    let fresh = (Utc::now() - Duration::seconds(5)).to_rfc3339();
    let _snapshot = server
        .mock("GET", "/api/v1/agent/deployments/deployment_ok")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(snapshot_body("deployment_ok", json!(fresh)))
        .create();

    // The pre-flight check is refused by the agent…
    let _check_enqueue = server
        .mock("POST", "/api/v1/agent/commands/enqueue")
        .match_header("authorization", "Bearer tok")
        .match_body(Matcher::PartialJson(
            json!({ "command_type": "check_connections" }),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(command_body("cmd_check", "deployment_ok", "queued"))
        .create();
    let _check_status = server
        .mock("GET", "/api/v1/commands/deployment_ok/cmd_check")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd_check",
                    "deployment_hash": "deployment_ok",
                    "type": "check_connections",
                    "status": "failed",
                    "error": {
                        "message": "Command validation failed: program 'check_connections' is not allowed"
                    },
                }
            })
            .to_string(),
        )
        .create();

    // …and the deploy itself goes through.
    let _deploy_enqueue = server
        .mock("POST", "/api/v1/agent/commands/enqueue")
        .match_header("authorization", "Bearer tok")
        .match_body(Matcher::PartialJson(
            json!({ "command_type": "deploy_app" }),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(command_body("cmd_deploy", "deployment_ok", "queued"))
        .create();
    let _deploy_status = server
        .mock("GET", "/api/v1/commands/deployment_ok/cmd_deploy")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "command_id": "cmd_deploy",
                    "deployment_hash": "deployment_ok",
                    "type": "deploy_app",
                    "status": "completed",
                    "result": { "status": "ok" },
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
            "app",
            "--image",
            "trydirect/hello:0.1.1",
            "--force",
            "--deployment",
            "deployment_ok",
        ])
        .assert()
        .stderr(
            predicate::str::contains("the agent refused the command").and(
                predicate::str::contains("program 'check_connections' is not allowed"),
            ),
        )
        .stderr(predicate::str::contains("agent could not verify active connections").not());
}
