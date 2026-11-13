//! Integration tests for `stacker install <template>`.
//!
//! These test the guard that prevents `stacker install` from running
//! inside an already-deployed project directory.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn write_stacker_yml(dir: &TempDir, deploy_target: &str) {
    let config = format!(
        r#"
name: test-project
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: {}
"#,
        deploy_target
    );
    fs::write(dir.path().join("stacker.yml"), config).unwrap();
}

fn write_cloud_lock(dir: &TempDir) {
    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).unwrap();
    let lock = r#"
target: cloud
server_ip: 203.0.113.42
server_name: test-server
deployment_id: 1
project_id: 1
cloud_id: 1
project_name: test-project
stacker_email: test@example.com
deployed_at: 2026-07-01T12:00:00+00:00
"#;
    fs::write(stacker_dir.join("deployment-cloud.lock"), lock).unwrap();
}

#[test]
fn test_install_refuses_existing_deployed_project() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "cloud");
    write_cloud_lock(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already contains a deployed project",
        ))
        .stderr(predicate::str::contains("stacker service add"))
        .stderr(predicate::str::contains("stacker agent deploy-app"));
}

#[test]
fn test_install_refuses_existing_local_deployed_project() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "local");
    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).unwrap();
    let lock = r#"
target: local
server_ip: 127.0.0.1
deployed_at: 2026-07-01T12:00:00+00:00
"#;
    fs::write(stacker_dir.join("deployment-local.lock"), lock).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already contains a deployed project",
        ));
}

#[test]
fn test_install_allowed_with_stacker_yml_but_no_lock() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "cloud");

    // Should NOT hit the "already contains a deployed project" guard.
    // It will fail for a different reason (no credentials / needs login),
    // but crucially not with our guard message.
    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains a deployed project").not());
}

#[test]
fn test_install_allowed_in_empty_directory() {
    let dir = TempDir::new().unwrap();

    // No stacker.yml, no lock — should proceed past our guard
    // and fail for a different reason (needs login / no credentials).
    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains a deployed project").not());
}

// ── flags on a template the server holds (BUGS.md §31) ────────────────────
//
// For such a template install builds no deploy form, so `--domain`, `--set`,
// `--key`, `--region` and `--size` reached no file: the written stacker.yml
// kept the template's defaults and a later `stacker deploy` never saw them.

fn write_credentials(config_home: &TempDir, server_url: &str) {
    let creds = stacker::cli::credentials::StoredCredentials {
        access_token: "tok".to_string(),
        refresh_token: Some("rtok".to_string()),
        token_type: "Bearer".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        email: Some("user@example.com".to_string()),
        server_url: Some(server_url.to_string()),
        org: None,
        domain: None,
    };
    let cred_dir = config_home.path().join("stacker");
    fs::create_dir_all(&cred_dir).unwrap();
    fs::write(
        cred_dir.join("credentials.json"),
        serde_json::to_vec(&creds).unwrap(),
    )
    .unwrap();
}

/// Mock the two calls a server-held template install makes, returning
/// `stack_definition` as the stored template.
fn mock_server_held_template(
    server: &mut mockito::Server,
    stack_definition: serde_json::Value,
) -> Vec<mockito::Mock> {
    let template = serde_json::json!({ "slug": "umami", "name": "Umami" });
    vec![
        server
            .mock("GET", "/api/templates/umami")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::json!({ "item": { "template": template } }).to_string())
            .create(),
        server
            .mock("POST", "/api/templates/umami/install")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({ "item": {
                    "project": {
                        "id": 816, "name": "umami", "user_id": "u1", "metadata": {},
                        "created_at": "2026-11-12T00:00:00Z",
                        "updated_at": "2026-11-12T00:00:00Z"
                    },
                    "template": template,
                    "latest_version": { "stack_definition": stack_definition },
                    "deployment_id": null
                }})
                .to_string(),
            )
            .create(),
    ]
}

fn umami_definition(cloud: Option<serde_json::Value>) -> serde_json::Value {
    let mut deploy = serde_json::json!({
        "target": "server",
        "server": { "host": "203.0.113.10", "user": "root", "port": 22 }
    });
    if let Some(cloud) = cloud {
        deploy["cloud"] = cloud;
    }
    serde_json::json!({
        "name": "umami",
        "app": {
            "type": "node",
            "image": "ghcr.io/umami-software/umami:postgresql-latest",
            "ports": ["3000:3000"]
        },
        "deploy": deploy,
        "install": { "inputs": { "commonDomain": "example.com" } }
    })
}

#[test]
fn test_install_writes_flags_into_stacker_yml_for_server_held_template() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    let mut server = mockito::Server::new();
    write_credentials(&config_home, &server.url());
    let _mocks = mock_server_held_template(
        &mut server,
        umami_definition(Some(serde_json::json!({
            "provider": "hetzner", "region": "fsn1", "size": "cpx22", "key": null
        }))),
    );

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args([
            "install",
            "umami",
            "--force",
            "--domain",
            "Umami.Example.com",
            "--set",
            "admin_email=you@example.com",
            "--key",
            "htz-10",
            "--region",
            "nbg1",
            "--size",
            "cx23",
        ])
        .assert()
        .success();

    let written = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&written).unwrap();
    let inputs = &doc["install"]["inputs"];
    assert_eq!(
        inputs["commonDomain"].as_str(),
        Some("umami.example.com"),
        "{written}"
    );
    assert_eq!(
        inputs["admin_email"].as_str(),
        Some("you@example.com"),
        "{written}"
    );
    let cloud = &doc["deploy"]["cloud"];
    assert_eq!(cloud["key"].as_str(), Some("htz-10"), "{written}");
    assert_eq!(cloud["region"].as_str(), Some("nbg1"), "{written}");
    assert_eq!(cloud["size"].as_str(), Some("cx23"), "{written}");
    assert_eq!(cloud["provider"].as_str(), Some("hetzner"), "{written}");
}

#[test]
fn test_install_says_which_cloud_flags_it_could_not_write() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    let mut server = mockito::Server::new();
    write_credentials(&config_home, &server.url());
    // No deploy.cloud block and no --provider: a block without a provider
    // does not parse, so the flag cannot be written and must not be dropped
    // in silence.
    let _mocks = mock_server_held_template(&mut server, umami_definition(None));

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["install", "umami", "--force", "--region", "nbg1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("--region").and(predicate::str::contains("deploy.cloud")));

    let written = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&written).unwrap();
    assert!(doc["deploy"]["cloud"].is_null(), "{written}");
}
