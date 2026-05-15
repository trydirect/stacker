use assert_cmd::Command;
use chrono::{Duration, Utc};
use mockito::{Matcher, Server};
use predicates::prelude::*;
use stacker::cli::credentials::StoredCredentials;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn create_test_jwt(exp: i64) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({"sub": "user-1", "email": "user@example.com", "exp": exp});

    format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(payload.to_string())
    )
}

#[test]
fn connect_handoff_account_saves_credentials_without_project_hydration() {
    let mut server = Server::new();
    let project_dir = TempDir::new().expect("project dir");
    let config_home = TempDir::new().expect("config home");
    let credentials_path = config_home.path().join("stacker").join("credentials.json");
    let jwt = create_test_jwt((Utc::now() + Duration::hours(6)).timestamp());

    let mock = server
        .mock("POST", "/api/v1/handoff/resolve")
        .match_header(
            "content-type",
            Matcher::Regex("application/json.*".to_string()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "_status": "OK",
                "msg": "CLI handoff resolved",
                "item": {
                    "kind": "account",
                    "version": 1,
                    "expires_at": (Utc::now() + Duration::hours(2)).to_rfc3339(),
                    "project": {
                        "id": 0,
                        "name": "user@example.com",
                        "identity": "user@example.com"
                    },
                    "deployment": {
                        "id": 0,
                        "hash": "",
                        "target": "account",
                        "status": "ready"
                    },
                    "lockfile": {},
                    "credentials": {
                        "access_token": jwt,
                        "token_type": "Bearer",
                        "expires_at": (Utc::now() + Duration::hours(6)).to_rfc3339(),
                        "email": "user@example.com",
                        "server_url": server.url()
                    }
                }
            })
            .to_string(),
        )
        .create();

    stacker_cmd()
        .current_dir(project_dir.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args([
            "connect",
            "--handoff",
            &format!("{}/handoff#account-token", server.url()),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Signed in to Stacker CLI"));

    mock.assert();
    assert!(credentials_path.exists());
    assert!(!project_dir.path().join("stacker.yml").exists());
    assert!(
        stacker::cli::deployment_lock::DeploymentLock::load(project_dir.path())
            .expect("load deployment lock")
            .is_none()
    );

    let saved: StoredCredentials =
        serde_json::from_slice(&std::fs::read(credentials_path).expect("read saved credentials"))
            .expect("deserialize saved credentials");
    assert_eq!(saved.email.as_deref(), Some("user@example.com"));
    assert_eq!(saved.token_type, "Bearer");
    assert!(
        saved.expires_at >= Utc::now() + Duration::hours(5),
        "saved credential should keep the underlying bearer expiry"
    );
}
