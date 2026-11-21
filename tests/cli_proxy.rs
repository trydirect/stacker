//! Integration tests for `stacker proxy`.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_proxy_add_generates_nginx_block() {
    stacker_cmd()
        .args([
            "proxy",
            "add",
            "example.com",
            "--upstream",
            "http://app:3000",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("server_name").or(predicate::str::contains("example.com")),
        );
}

#[test]
fn test_proxy_add_with_ssl() {
    stacker_cmd()
        .args([
            "proxy",
            "add",
            "secure.example.com",
            "--upstream",
            "http://app:3000",
            "--ssl",
            "auto",
        ])
        .assert()
        .success();
}

#[test]
fn test_proxy_detect_help() {
    stacker_cmd()
        .args(["proxy", "detect", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Detect"));
}

#[test]
fn test_proxy_add_help_shows_options() {
    stacker_cmd()
        .args(["proxy", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--upstream"))
        .stdout(predicate::str::contains("--ssl"));
}

// BUGS.md #37: `--type` names the proxy flavour instead of silently flipping
// the config to nginx-proxy-manager; the retired `nginx` value is refused.
#[test]
fn test_proxy_add_accepts_type_flag() {
    stacker_cmd()
        .args(["proxy", "add", "example.com", "--type", "caddy", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--type"));
}

#[test]
fn test_proxy_add_rejects_unsupported_type() {
    stacker_cmd()
        .args(["proxy", "add", "example.com", "--type", "nginx"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("possible values"));
}
