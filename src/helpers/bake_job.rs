//! Bake started from the API (`POST /api/admin/bake`) instead of the `bake`
//! binary.
//!
//! The binary needs the operator to type the box's IP, its SSH key, the
//! version and the health URL. Here they come from what stacker already
//! holds: the deployment's server row (IP, SSH user and port), the server's
//! SSH key in Vault, and the template's latest approved version. These pure
//! helpers check what the box reports before anything on it is changed.

/// The stack's project directory on the build box, from
/// `docker compose ls --all --format json`.
///
/// The deploy puts each stack in its own directory under the deploy root;
/// platform services (status panel, proxy manager) have their own. Anything
/// but exactly one project directory is refused: sanitizing the wrong one
/// would publish the author's secrets.
pub fn project_dir_from_compose_ls(stdout: &str, platform_dirs: &[&str]) -> Result<String, String> {
    let projects: Vec<serde_json::Value> = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("could not read `docker compose ls` output: {e}"))?;

    let mut dirs: Vec<String> = projects
        .iter()
        .filter_map(|p| p.get("ConfigFiles").and_then(|v| v.as_str()))
        .filter_map(|files| files.split(',').next())
        .filter_map(|file| file.trim().rsplit_once('/').map(|(dir, _)| dir.to_string()))
        .filter(|dir| !platform_dirs.contains(&dir.as_str()))
        .collect();
    dirs.sort();
    dirs.dedup();

    match dirs.as_slice() {
        [dir] if is_safe_dir(dir) => Ok(dir.clone()),
        [dir] => Err(format!("refusing the stack directory `{dir}`: not a plain absolute path")),
        [] => Err("no stack is deployed on the build box (only platform services)".to_string()),
        many => Err(format!(
            "more than one stack is deployed on the build box: {}. Bake needs a box with one stack.",
            many.join(", ")
        )),
    }
}

/// An absolute path of letters, digits and `/ _ . -` only: it goes into a
/// shell command on the box.
fn is_safe_dir(dir: &str) -> bool {
    dir.starts_with('/')
        && !dir.contains("..")
        && dir
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-'))
}

/// Every service of the stack is up, from `docker compose ps --all --format
/// json` (one JSON object per line in compose v2.21+, a JSON array before).
///
/// Running without a healthcheck or healthy counts as up; a one-shot job that
/// exited 0 (migrations, init) is done, not down.
pub fn check_compose_services(stdout: &str) -> Result<(), String> {
    let text = stdout.trim();
    let services: Vec<serde_json::Value> = if text.starts_with('[') {
        serde_json::from_str(text)
            .map_err(|e| format!("could not read `docker compose ps`: {e}"))?
    } else {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("could not read `docker compose ps`: {e}"))?
    };
    if services.is_empty() {
        return Err("the stack has no services on the build box".to_string());
    }

    let mut down = Vec::new();
    for service in &services {
        let field = |name: &str| service.get(name).and_then(|v| v.as_str()).unwrap_or("");
        let name = field("Service");
        let state = field("State");
        let health = field("Health");
        let exit_code = service
            .get("ExitCode")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        let up = match state {
            "running" => health.is_empty() || health == "healthy",
            "exited" => exit_code == 0,
            _ => false,
        };
        if !up {
            let detail = if health.is_empty() {
                state.to_string()
            } else {
                format!("{state}, {health}")
            };
            down.push(format!("{name} ({detail})"));
        }
    }
    if down.is_empty() {
        Ok(())
    } else {
        Err(format!("services not up: {}", down.join(", ")))
    }
}

/// The health URL must point at the build box itself: the bake runs on the
/// stacker backend, so any other host would make it a request the caller
/// chose from inside the platform's network.
pub fn check_health_url(url: &str, server_ip: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("health_url is not a URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("health_url must be http or https".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("health_url must not carry credentials".to_string());
    }
    match parsed.host_str() {
        Some(host) if host == server_ip => Ok(()),
        _ => Err(format!(
            "health_url must point at the build box ({server_ip})"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORM: &[&str] = &[
        "/home/trydirect/statuspanel",
        "/home/trydirect/nginx_proxy_manager",
    ];

    #[test]
    fn the_stack_directory_is_found_next_to_platform_services() {
        let ls = r#"[
            {"Name":"statuspanel","Status":"running(1)","ConfigFiles":"/home/trydirect/statuspanel/docker-compose.yml"},
            {"Name":"ai-workflows-v2-12","Status":"running(4)","ConfigFiles":"/home/trydirect/ai-workflows-v2-12/docker-compose.yml"}
        ]"#;
        assert_eq!(
            project_dir_from_compose_ls(ls, PLATFORM).unwrap(),
            "/home/trydirect/ai-workflows-v2-12"
        );
    }

    #[test]
    fn several_config_files_of_one_project_point_at_its_directory() {
        let ls = r#"[{"Name":"app","Status":"running(2)","ConfigFiles":"/home/trydirect/app/docker-compose.yml,/home/trydirect/app/docker-compose.override.yml"}]"#;
        assert_eq!(
            project_dir_from_compose_ls(ls, PLATFORM).unwrap(),
            "/home/trydirect/app"
        );
    }

    #[test]
    fn no_stack_or_two_stacks_are_refused() {
        let only_platform = r#"[{"Name":"statuspanel","Status":"running(1)","ConfigFiles":"/home/trydirect/statuspanel/docker-compose.yml"}]"#;
        assert!(project_dir_from_compose_ls(only_platform, PLATFORM).is_err());

        let two = r#"[
            {"Name":"a","Status":"running(1)","ConfigFiles":"/home/trydirect/a/docker-compose.yml"},
            {"Name":"b","Status":"running(1)","ConfigFiles":"/home/trydirect/b/docker-compose.yml"}
        ]"#;
        let err = project_dir_from_compose_ls(two, PLATFORM).unwrap_err();
        assert!(
            err.contains("/home/trydirect/a") && err.contains("/home/trydirect/b"),
            "{err}"
        );

        assert!(project_dir_from_compose_ls("not json", PLATFORM).is_err());
    }

    #[test]
    fn a_directory_unsafe_for_a_shell_command_is_refused() {
        // The directory is put into `cd <dir> && ...` on the box.
        for dir in [
            "/home/trydirect/a b",
            "/home/trydirect/a;rm -rf /",
            "/home/trydirect/$(id)",
            "relative/dir",
        ] {
            let ls = format!(
                r#"[{{"Name":"x","Status":"running(1)","ConfigFiles":"{dir}/docker-compose.yml"}}]"#
            );
            assert!(project_dir_from_compose_ls(&ls, PLATFORM).is_err(), "{dir}");
        }
    }

    #[test]
    fn services_running_healthy_or_finished_are_up() {
        let ps = "\
{\"Service\":\"n8n\",\"State\":\"running\",\"Health\":\"healthy\",\"ExitCode\":0}
{\"Service\":\"redis\",\"State\":\"running\",\"Health\":\"\",\"ExitCode\":0}
{\"Service\":\"migrate\",\"State\":\"exited\",\"Health\":\"\",\"ExitCode\":0}";
        assert_eq!(check_compose_services(ps), Ok(()));

        let array = r#"[{"Service":"n8n","State":"running","Health":"healthy","ExitCode":0}]"#;
        assert_eq!(check_compose_services(array), Ok(()));
    }

    #[test]
    fn a_starting_unhealthy_crashed_or_missing_service_fails_the_gate() {
        for (ps, culprit) in [
            (
                r#"{"Service":"db","State":"running","Health":"starting","ExitCode":0}"#,
                "db",
            ),
            (
                r#"{"Service":"db","State":"running","Health":"unhealthy","ExitCode":0}"#,
                "db",
            ),
            (
                r#"{"Service":"api","State":"exited","Health":"","ExitCode":1}"#,
                "api",
            ),
            (
                r#"{"Service":"api","State":"restarting","Health":"","ExitCode":1}"#,
                "api",
            ),
        ] {
            let err = check_compose_services(ps).unwrap_err();
            assert!(err.contains(culprit), "{err}");
        }
        assert!(
            check_compose_services("").is_err(),
            "no services is not healthy"
        );
        assert!(check_compose_services("garbage").is_err());
    }

    #[test]
    fn the_health_url_must_point_at_the_build_box() {
        assert_eq!(
            check_health_url("http://203.0.113.7/health", "203.0.113.7"),
            Ok(())
        );
        assert_eq!(
            check_health_url("https://203.0.113.7:8443/api/ping", "203.0.113.7"),
            Ok(())
        );
        for url in [
            "http://198.51.100.1/health",
            "http://203.0.113.7.evil.example/health",
            "http://user@198.51.100.1/",
            "file:///etc/passwd",
            "http://localhost/health",
            "not a url",
        ] {
            assert!(check_health_url(url, "203.0.113.7").is_err(), "{url}");
        }
    }
}
