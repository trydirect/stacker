use assert_cmd::Command;
use predicates::prelude::*;
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
  deployment_hash: deployment_state_online
  environment: prod
env_file: docker/prod/.env
env:
  HOST: "0.0.0.0"
services:
  - name: device-api
    image: optimum/device-api
    environment:
      DATABASE_URL: postgres://secret-value
      RUST_LOG: debug
environments:
  prod:
    compose_file: docker/prod/compose.yml
    env_file: docker/prod/.env
"#;
    fs::create_dir_all(dir.path().join("docker/prod")).expect("create docker/prod");
    fs::write(dir.path().join("stacker.yml"), config).expect("write stacker.yml");
    fs::write(dir.path().join("index.html"), "<h1>Hello</h1>").expect("write index.html");
    fs::write(dir.path().join("docker/prod/.env"), "HOST=0.0.0.0\n").expect("write env");
    fs::write(
        dir.path().join("docker/prod/compose.yml"),
        "services:\n  device-api:\n    image: optimum/device-api\n",
    )
    .expect("write compose");
}

#[test]
fn explain_env_json_outputs_redacted_provenance() {
    let dir = TempDir::new().unwrap();
    write_stacker_config(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["explain", "env", "device-api", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains("\"appCode\": \"device-api\""))
                .and(predicate::str::contains(
                    "\"runtimeEnvPath\": \"/home/trydirect/project/.env\"",
                ))
                .and(predicate::str::contains("DATABASE_URL"))
                .and(predicate::str::contains("secret-value").not()),
        );
}

#[test]
fn explain_topology_json_outputs_paths_and_services() {
    let dir = TempDir::new().unwrap();
    write_stacker_config(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["explain", "topology", "--json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"schemaVersion\": \"v1alpha1\"")
                .and(predicate::str::contains("\"target\": \"cloud\""))
                .and(predicate::str::contains(
                    "\"runtimeComposePath\": \"/home/trydirect/project/docker-compose.yml\"",
                ))
                .and(predicate::str::contains("\"code\": \"device-api\"")),
        );
}

#[test]
fn explain_env_missing_service_returns_typed_error() {
    let dir = TempDir::new().unwrap();
    write_stacker_config(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["explain", "env", "missing-service", "--json"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("invalid_request")
                .and(predicate::str::contains("remediationClass"))
                .and(predicate::str::contains("configuration")),
        );
}
