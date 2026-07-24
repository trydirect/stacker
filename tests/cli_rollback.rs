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

fn write_stacker_config(dir: &TempDir) {
    let config = r#"
name: local-name
project:
  identity: remote-project
app:
  type: static
  path: "."
deploy:
  target: cloud
"#;
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
fn rollback_requires_confirmation() {
    let dir = TempDir::new().unwrap();
    write_stacker_config(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["rollback", "--version", "1.0.0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("confirm").or(predicate::str::contains("Rollback")));
}

#[test]
fn rollback_posts_version_to_project_rollback_endpoint() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir);

    let mut server = Server::new();
    write_credentials(&config_home, &format!("{}/auth/login", server.url()));

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
        .match_body(Matcher::Json(json!({ "version": "1.0.0" })))
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
        .args(["rollback", "--version", "1.0.0", "--confirm"])
        .assert()
        .success();

    list_projects.assert();
    rollback.assert();
}
