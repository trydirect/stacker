use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn completion_outputs_script() {
    stacker_cmd()
        .args(["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stacker"));
}

#[test]
fn required_args_are_enforced() {
    let cases: &[&[&str]] = &[
        &["completion"],
        &["ssh-key", "generate"],
        &["ssh-key", "show"],
        &["ssh-key", "upload"],
        &["ssh-key", "inject"],
        &["service", "remove"],
        &["secrets", "set"],
        &["secrets", "get"],
        &["secrets", "delete"],
        &["ci", "export"],
        &["ci", "validate"],
        &["agent", "restart"],
        &["agent", "deploy-app"],
        &["agent", "remove-app"],
        &["agent", "configure-proxy"],
        &["agent", "exec"],
        &["proxy", "add"],
    ];

    for args in cases {
        stacker_cmd()
            .args(args.iter().copied())
            .assert()
            .failure()
            .stderr(predicate::str::contains("required"));
    }
}
