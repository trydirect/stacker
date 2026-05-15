use assert_cmd::Command;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_version_matches_formatted_build_version() {
    stacker_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("{}\n", stacker::version::display_version()));
}
