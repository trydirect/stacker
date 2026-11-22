//! Step 4 of the deployment/agent-identity plan — BUGS.md #59 (and the
//! direction in #58): `agent install` means "make sure an agent is here",
//! not "cut a new release". When the project already has a deployment and
//! this machine can reach the server, the agent is (re)registered on it —
//! `POST /api/v1/agent/link` creates or returns the registration — and the
//! deployment hash does not change, so every later command keeps working.
//!
//! `--new-deployment` is the escape hatch that forces the deploy path.

use assert_cmd::Command;
use chrono::{Duration, Utc};
use mockito::{Matcher, Server};
use predicates::prelude::*;
use serde_json::json;
use stacker::cli::credentials::StoredCredentials;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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

/// An `ssh` that always succeeds: the remote commands (install script, .env
/// write, daemon start) are the status panel's, not this test's subject.
fn fake_ssh(root: &TempDir) -> PathBuf {
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin dir");
    let script = bin.join("ssh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write ssh stub");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod ssh stub");
    bin
}

fn path_with(bin: &std::path::Path) -> String {
    let old = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bin.display(), old)
}

/// A cloud project with a deployment and a server the CLI can reach — the
/// state that must NOT grow a second deployment on `agent install`.
fn setup_project(dir: &TempDir, key_path: &PathBuf) {
    fs::write(
        dir.path().join("stacker.yml"),
        "name: hello\nversion: \"1.0\"\napp:\n  type: node\n  path: \".\"\n  image: trydirect/hello:0.1.0\ndeploy:\n  target: cloud\n",
    )
    .expect("write stacker.yml");
    fs::write(dir.path().join("index.html"), "<h1>x</h1>").expect("write index.html");
    fs::write(key_path, "not-a-real-key").expect("write ssh key");

    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).expect("create .stacker");
    fs::write(
        stacker_dir.join("deployment-cloud.lock"),
        format!(
            "target: cloud\nserver_ip: 203.0.113.5\nssh_user: root\nssh_port: 22\nssh_key: {}\nserver_name: hello-4282\ndeployment_id: 1546\ndeployment_hash: deployment_live\nproject_id: 822\nproject_name: hello\nstacker_email: user@example.com\ndeployed_at: 2026-11-21T10:00:00+00:00\n",
            key_path.display()
        ),
    )
    .expect("write lock");
}

fn mount_common_mocks(server: &mut Server) {
    server
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
    server
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
    server
        .mock("GET", "/api/v1/deployments")
        .match_query(Matcher::Any)
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "list": [{
                    "id": 1546,
                    "project_id": 822,
                    "deployment_hash": "deployment_live",
                    "status": "completed",
                    "status_message": null,
                    "created_at": "2026-11-21T10:00:00Z",
                    "updated_at": "2026-11-21T10:05:00Z",
                }]
            })
            .to_string(),
        )
        .create();
}

#[test]
fn install_reregisters_on_the_existing_deployment_instead_of_creating_one() {
    let mut server = Server::new();
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("id_ed25519");
    let bin = fake_ssh(&dir);
    write_credentials(&config_home, &server.url());
    setup_project(&dir, &key_path);
    mount_common_mocks(&mut server);

    let link = server
        .mock("POST", "/api/v1/agent/link")
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "agent_id": "a1",
                "agent_token": "t1",
                "deployment_hash": "deployment_live",
            })
            .to_string(),
        )
        .create();

    let deploy = server
        .mock(
            "POST",
            Matcher::Regex(r"^/api/v1/project/822/deploy(/\d+)?$".to_string()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({ "_status": "OK", "id": 822 }).to_string())
        .expect(0)
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("PATH", path_with(&bin))
        .args(["agent", "install"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Agent linked (id: a1)")
                .and(predicate::str::contains("the deployment is unchanged")),
        )
        .stdout(predicate::str::contains("Status Panel deploy queued").not());

    link.assert();
    deploy.assert();
}

#[test]
fn new_deployment_flag_forces_the_deploy_path() {
    let mut server = Server::new();
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let key_path = dir.path().join("id_ed25519");
    let bin = fake_ssh(&dir);
    write_credentials(&config_home, &server.url());
    setup_project(&dir, &key_path);
    mount_common_mocks(&mut server);

    let link = server
        .mock("POST", "/api/v1/agent/link")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({ "_status": "OK", "item": {} }).to_string())
        .expect(0)
        .create();

    let deploy = server
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
        .expect(1)
        .create();

    // Step 3's lock recording fetches the created deployment's hash.
    let _created = server
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
                    "deployment_hash": "deployment_new",
                    "status": "queued",
                    "status_message": null,
                    "created_at": "2026-11-22T10:00:00Z",
                    "updated_at": "2026-11-22T10:00:00Z",
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("PATH", path_with(&bin))
        .args(["agent", "install", "--new-deployment"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status Panel deploy queued"));

    link.assert();
    deploy.assert();
}
