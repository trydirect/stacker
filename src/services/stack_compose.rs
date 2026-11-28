//! Where a deployment's stack compose actually lives on the server.
//!
//! Stacker used to guess `/home/trydirect/<name>-<id>/docker-compose.yml`, but a
//! stack can run elsewhere (a legacy `/home/trydirect/project`, a catalog role's
//! own directory). The status agent reads the compose labels of the project's
//! containers and reports `stack_compose: {dir, file}` in aggregate `health`
//! and `deploy_app` results; the report handler records it on the agent under
//! `system_info.stack_compose`, and deployment state shows it.

use crate::forms::status_panel::StackComposeLocation;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

/// Key under the agent's `system_info`.
pub const SYSTEM_INFO_KEY: &str = "stack_compose";

const MAX_PATH_LEN: usize = 4096;

/// The `stack_compose` a command result carries, if it is a pair of absolute
/// paths without control characters — the values end up in API responses.
pub fn from_report(result: &Value) -> Option<StackComposeLocation> {
    let location = result.get(SYSTEM_INFO_KEY)?;
    let dir = valid_path(location.get("dir")?.as_str()?)?;
    let file = valid_path(location.get("file")?.as_str()?)?;
    Some(StackComposeLocation { dir, file })
}

/// `system_info` with `stack_compose` set, or `None` when nothing changes or
/// the existing value is not a JSON object (it is then left untouched rather
/// than replaced).
pub fn merge_into_system_info(
    system_info: Option<&Value>,
    location: &StackComposeLocation,
    reported_at: DateTime<Utc>,
) -> Option<Value> {
    let mut merged = match system_info {
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => return None,
    };
    if from_system_info(system_info).as_ref() == Some(location) {
        return None;
    }
    merged.insert(
        SYSTEM_INFO_KEY.to_string(),
        json!({
            "dir": location.dir,
            "file": location.file,
            "reported_at": reported_at.to_rfc3339(),
        }),
    );
    Some(Value::Object(merged))
}

/// The location recorded on the agent, if any.
pub fn from_system_info(system_info: Option<&Value>) -> Option<StackComposeLocation> {
    from_report(system_info?)
}

/// `<dir>/.env` — the file Compose reads `${VAR}` from.
pub fn env_path(location: &StackComposeLocation) -> String {
    format!("{}/.env", location.dir.trim_end_matches('/'))
}

/// The stack directory's name, e.g. `project` for `/home/trydirect/project`.
pub fn stack_dir_name(location: &StackComposeLocation) -> String {
    location
        .dir
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn valid_path(value: &str) -> Option<String> {
    let value = value.trim();
    let ok = value.starts_with('/')
        && value.len() <= MAX_PATH_LEN
        && !value.chars().any(char::is_control);
    ok.then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> StackComposeLocation {
        StackComposeLocation {
            dir: "/home/trydirect/project".to_string(),
            file: "/home/trydirect/project/docker-compose.yml".to_string(),
        }
    }

    #[test]
    fn from_report_reads_absolute_paths() {
        let result = json!({
            "type": "all_health",
            "stack_compose": {"dir": "/home/trydirect/project", "file": "/home/trydirect/project/docker-compose.yml"}
        });
        assert_eq!(from_report(&result), Some(project()));
    }

    #[test]
    fn from_report_rejects_relative_or_control_characters_or_missing() {
        for location in [
            json!({"dir": "project", "file": "/x/docker-compose.yml"}),
            json!({"dir": "/x", "file": "docker-compose.yml"}),
            json!({"dir": "/x\n/y", "file": "/x/docker-compose.yml"}),
            json!({"dir": "/x"}),
            json!("nope"),
        ] {
            assert_eq!(from_report(&json!({ "stack_compose": location })), None);
        }
        assert_eq!(from_report(&json!({"type": "all_health"})), None);
    }

    #[test]
    fn merge_keeps_other_system_info_keys() {
        let now = Utc::now();
        let existing = json!({"hostname": "srv", "os": "linux"});

        let merged = merge_into_system_info(Some(&existing), &project(), now).expect("changed");

        assert_eq!(merged["hostname"], "srv");
        assert_eq!(merged["os"], "linux");
        assert_eq!(merged["stack_compose"]["dir"], "/home/trydirect/project");
        assert_eq!(merged["stack_compose"]["reported_at"], now.to_rfc3339());
    }

    #[test]
    fn merge_starts_an_object_from_nothing_and_skips_non_objects() {
        assert!(merge_into_system_info(None, &project(), Utc::now()).is_some());
        assert!(merge_into_system_info(Some(&Value::Null), &project(), Utc::now()).is_some());
        assert_eq!(
            merge_into_system_info(Some(&json!("fingerprint")), &project(), Utc::now()),
            None
        );
    }

    #[test]
    fn merge_is_none_when_the_location_is_unchanged() {
        let recorded = merge_into_system_info(None, &project(), Utc::now()).unwrap();

        assert_eq!(
            merge_into_system_info(Some(&recorded), &project(), Utc::now()),
            None
        );
    }

    #[test]
    fn env_path_and_dir_name_follow_the_reported_dir() {
        assert_eq!(env_path(&project()), "/home/trydirect/project/.env");
        assert_eq!(stack_dir_name(&project()), "project");
    }
}
