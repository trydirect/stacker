//! Regression for BUGS.md #59: `stacker agent install` creates a deployment
//! and must **record it** — the way a deploy does. The install printed
//! `Deployment ID: 1547` while the lock kept naming 1546, so the very next
//! `stacker agent status` resolved the previous deployment and waited out
//! its timeout against an agent that no longer runs.

use assert_cmd::Command;
use chrono::{Duration, Utc};
use mockito::{Matcher, Server};
use predicates::prelude::*;
use serde_json::json;
use stacker::cli::credentials::StoredCredentials;
use std::fs;
use std::path::PathBuf;
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

/// A cloud project whose pin still points at the *previous* deployment —
/// the state every `agent install` left behind in BUGS.md #59.
fn setup_project(dir: &TempDir) {
    fs::write(
        dir.path().join("stacker.yml"),
        "name: hello\nversion: \"1.0\"\napp:\n  type: node\n  path: \".\"\n  image: trydirect/hello:0.1.0\ndeploy:\n  target: cloud\n  deployment_hash: deployment_old\n",
    )
    .expect("write stacker.yml");
    fs::write(dir.path().join("index.html"), "<h1>x</h1>").expect("write index.html");
}

fn lock_files(dir: &TempDir) -> Vec<PathBuf> {
    let stacker_dir = dir.path().join(".stacker");
    if !stacker_dir.exists() {
        return Vec::new();
    }
    fs::read_dir(&stacker_dir)
        .expect("read .stacker")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("deployment-") && n.ends_with(".lock"))
                .unwrap_or(false)
        })
        .collect()
}

#[test]
fn agent_install_records_the_deployment_it_created() {
    let mut server = Server::new();
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    write_credentials(&config_home, &server.url());
    setup_project(&dir);

    let _projects = server
        .mock("GET", "/api/v1/project")
        .match_query(Matcher::Any)
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "list": [{
                    "id": 822,
                    "name": "hello",
                    "user_id": "test_user_id",
                    "metadata": {},
                    "created_at": "2026-11-20T10:00:00Z",
                    "updated_at": "2026-11-20T10:00:00Z",
                }]
            })
            .to_string(),
        )
        .create();

    let _servers = server
        .mock("GET", "/server")
        .match_query(Matcher::Any)
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "list": [{
                    "id": 290,
                    "user_id": "test_user_id",
                    "project_id": 822,
                    "cloud_id": 10,
                    "name": "hello-4282",
                    "srv_ip": "203.0.113.5",
                    "ssh_port": 22,
                    "ssh_user": "root",
                }]
            })
            .to_string(),
        )
        .create();

    let _deploy = server
        .mock(
            "POST",
            Matcher::Regex(r"^/api/v1/project/822/deploy(/\d+)?$".to_string()),
        )
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Install deployment started",
                "id": 822,
                "meta": { "deployment_id": 1547 },
            })
            .to_string(),
        )
        .create();

    // The deployment the install created — its hash is what the lock must carry.
    let _deployment = server
        .mock("GET", "/api/v1/deployments/1547")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "item": {
                    "id": 1547,
                    "project_id": 822,
                    "deployment_hash": "deployment_fresh",
                    "status": "queued",
                    "status_message": null,
                    "created_at": "2026-11-21T18:00:00Z",
                    "updated_at": "2026-11-21T18:00:00Z",
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["agent", "install"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Deployment context saved to"));

    let locks = lock_files(&dir);
    assert_eq!(locks.len(), 1, "the install must write one lock: {locks:?}");
    let lock = fs::read_to_string(&locks[0]).expect("read lock");
    assert!(
        lock.contains("deployment_hash: deployment_fresh"),
        "the lock must name the deployment the install created:\n{lock}"
    );

    // The pin is the user's act and is not rewritten — but its staleness is
    // said out loud, with the one command that moves it.
    let second = stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["agent", "install"])
        .output()
        .expect("second install output");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains("your pin points at deployment_old"),
        "a stale pin must be named:\n{stderr}"
    );
    assert!(
        stderr.contains("stacker deployment use deployment_fresh"),
        "the fix must be one copyable line:\n{stderr}"
    );
}
