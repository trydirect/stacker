use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_connect_help_shows_handoff_usage() {
    stacker_cmd()
        .args(["connect", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("connect --handoff")
                .and(predicate::str::contains("Handoff token"))
        );
}
