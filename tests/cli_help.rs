use assert_cmd::Command;

fn assert_help(binary: &str, args: &[&str]) {
    Command::cargo_bin(binary)
        .expect("CLI binary not found")
        .args(args)
        .assert()
        .success();
}

#[test]
fn stacker_cli_help_matrix() {
    let commands: &[&[&str]] = &[
        &["--help"],
        &["login", "--help"],
        &["init", "--help"],
        &["deploy", "--help"],
        &["logs", "--help"],
        &["status", "--help"],
        &["destroy", "--help"],
        &["config", "--help"],
        &["config", "validate", "--help"],
        &["config", "show", "--help"],
        &["config", "example", "--help"],
        &["config", "fix", "--help"],
        &["config", "lock", "--help"],
        &["config", "unlock", "--help"],
        &["config", "setup", "--help"],
        &["config", "setup", "cloud", "--help"],
        &["config", "setup", "remote-payload", "--help"],
        &["ai", "--help"],
        &["ai", "ask", "--help"],
        &["proxy", "--help"],
        &["proxy", "add", "--help"],
        &["proxy", "detect", "--help"],
        &["list", "--help"],
        &["list", "projects", "--help"],
        &["list", "deployments", "--help"],
        &["list", "servers", "--help"],
        &["list", "ssh-keys", "--help"],
        &["list", "clouds", "--help"],
        &["ssh-key", "--help"],
        &["ssh-key", "generate", "--help"],
        &["ssh-key", "show", "--help"],
        &["ssh-key", "upload", "--help"],
        &["ssh-key", "inject", "--help"],
        &["service", "--help"],
        &["service", "add", "--help"],
        &["service", "remove", "--help"],
        &["service", "list", "--help"],
        &["resolve", "--help"],
        &["update", "--help"],
        &["completion", "--help"],
        &["secrets", "--help"],
        &["secrets", "set", "--help"],
        &["secrets", "get", "--help"],
        &["secrets", "list", "--help"],
        &["secrets", "delete", "--help"],
        &["secrets", "validate", "--help"],
        &["ci", "--help"],
        &["ci", "export", "--help"],
        &["ci", "validate", "--help"],
        &["agent", "--help"],
        &["agent", "health", "--help"],
        &["agent", "logs", "--help"],
        &["agent", "restart", "--help"],
        &["agent", "deploy-app", "--help"],
        &["agent", "remove-app", "--help"],
        &["agent", "configure-firewall", "--help"],
        &["agent", "configure-proxy", "--help"],
        &["agent", "list", "--help"],
        &["agent", "list", "apps", "--help"],
        &["agent", "list", "containers", "--help"],
        &["agent", "status", "--help"],
        &["agent", "history", "--help"],
        &["agent", "exec", "--help"],
        &["agent", "install", "--help"],
    ];

    for args in commands {
        assert_help("stacker-cli", args);
    }
}

#[cfg(feature = "explain")]
#[test]
fn console_help_matrix() {
    let commands: &[&[&str]] = &[
        &["--help"],
        &["app-client", "--help"],
        &["app-client", "new", "--help"],
        &["debug", "--help"],
        &["debug", "json", "--help"],
        &["debug", "casbin", "--help"],
        &["debug", "dockerhub", "--help"],
        &["mq", "--help"],
        &["mq", "listen", "--help"],
        &["agent", "--help"],
        &["agent", "rotate-token", "--help"],
        &["stacker", "--help"],
        &["stacker", "login", "--help"],
        &["stacker", "init", "--help"],
        &["stacker", "deploy", "--help"],
        &["stacker", "logs", "--help"],
        &["stacker", "status", "--help"],
        &["stacker", "destroy", "--help"],
        &["stacker", "config", "--help"],
        &["stacker", "config", "validate", "--help"],
        &["stacker", "config", "show", "--help"],
        &["stacker", "config", "example", "--help"],
        &["stacker", "config", "fix", "--help"],
        &["stacker", "config", "lock", "--help"],
        &["stacker", "config", "unlock", "--help"],
        &["stacker", "config", "setup", "--help"],
        &["stacker", "config", "setup", "cloud", "--help"],
        &["stacker", "config", "setup", "remote-payload", "--help"],
        &["stacker", "ai", "--help"],
        &["stacker", "ai", "ask", "--help"],
        &["stacker", "proxy", "--help"],
        &["stacker", "proxy", "add", "--help"],
        &["stacker", "proxy", "detect", "--help"],
        &["stacker", "resolve", "--help"],
        &["stacker", "update", "--help"],
    ];

    for args in commands {
        assert_help("console", args);
    }
}
