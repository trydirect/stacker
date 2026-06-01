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
fn deploy_plan_outputs_read_only_plan_json() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let plan = json!({
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
    });

    let mock = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment plan fetched",
                "item": plan
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["deploy", "--plan"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains("\"operation\": \"deploy\""))
                .and(predicate::str::contains("\"hasChanges\": false")),
        );

    mock.assert();
}

#[test]
fn agent_deploy_app_plan_outputs_scoped_plan_json() {
    let dir = TempDir::new().unwrap();
    let config_home = TempDir::new().unwrap();
    write_stacker_config(&dir, "deployment_state_online");

    let mut server = Server::new();
    write_credentials(&config_home, &server.url());

    let plan = json!({
        "schemaVersion": "v1alpha1",
        "deploymentHash": "deployment_state_online",
        "operation": "deploy_app",
        "target": "cloud",
        "fingerprint": "plan-deploy-app",
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
    });

    let mock = server
        .mock("GET", "/api/v1/deployments/deployment_state_online/plan")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("operation".into(), "deploy_app".into()),
            mockito::Matcher::UrlEncoded("target".into(), "cloud".into()),
            mockito::Matcher::UrlEncoded("appCode".into(), "upload".into()),
        ]))
        .match_header("authorization", "Bearer tok")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "_status": "OK",
                "msg": "Deployment plan fetched",
                "item": plan
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
            "--plan",
            "--deployment",
            "deployment_state_online",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains("\"operation\": \"deploy_app\""))
                .and(predicate::str::contains("\"appCode\": \"upload\"")),
        );

    mock.assert();
}
