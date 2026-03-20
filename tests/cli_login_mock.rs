use assert_cmd::Command;
use mockito::Server;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn login_uses_mocked_auth_endpoint() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/auth/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"access_token":"tok","refresh_token":"rtok","token_type":"Bearer","expires_in":3600}"#,
        )
        .create();

    let temp = TempDir::new().expect("temp dir");
    let credentials_path = temp.path().join("stacker").join("credentials.json");

    Command::cargo_bin("stacker-cli")
        .expect("stacker-cli binary not found")
        .env("XDG_CONFIG_HOME", temp.path())
        .args(["login", "--auth-url", &server.url()])
        .write_stdin("user@example.com\nsecret\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("Logged in as"));

    mock.assert();
    assert!(credentials_path.exists());
}
