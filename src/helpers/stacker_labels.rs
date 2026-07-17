use std::collections::HashMap;

pub const PROJECT_ID: &str = "my.stacker.project_id";
pub const TARGET: &str = "my.stacker.target";
pub const SCOPE: &str = "my.stacker.scope";
pub const SERVICE: &str = "my.stacker.service";
pub const DNS: &str = "my.stacker.dns";

pub const SCOPE_PROJECT: &str = "project";
pub const SCOPE_PLATFORM: &str = "platform";

/// Normalize a project/app name into the stable code used to identify a service
/// to the status-panel agent. Mirrors the deploy-time stack-code derivation so
/// the `my.stacker.service` label matches the app code the agent resolves by
/// (lowercase alphanumerics, runs of other characters collapsed to `-`).
pub fn sanitize_service_code(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "app-stack".to_string()
    } else {
        out
    }
}

pub fn insert_runtime_labels(
    labels: &mut HashMap<String, String>,
    project_id: Option<impl ToString>,
    target: Option<&str>,
    scope: &str,
    service: &str,
    dns: &str,
) {
    if let Some(project_id) = project_id {
        labels.insert(PROJECT_ID.to_string(), project_id.to_string());
    }
    if let Some(target) = target.filter(|value| !value.trim().is_empty()) {
        labels.insert(TARGET.to_string(), target.to_string());
    }
    labels.insert(SCOPE.to_string(), scope.to_string());
    labels.insert(SERVICE.to_string(), service.to_string());
    labels.insert(DNS.to_string(), dns.to_string());
}
