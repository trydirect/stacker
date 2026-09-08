pub(crate) mod hydration;
pub(crate) mod mapping;
pub(crate) mod sync;
pub(crate) mod upsert;
pub(crate) mod vault;

pub(crate) use mapping::{merge_project_app, project_app_from_post};
pub(crate) use sync::sync_project_level_apps_from_form;
pub(crate) use upsert::upsert_app_config_for_deploy;
pub(crate) use vault::{
    parse_registry_auth_config, store_configs_to_vault_from_params,
    store_registry_auth_command_to_vault, store_registry_auth_to_vault, REGISTRY_AUTH_VAULT_KEY,
};

/// Services installed by their own Ansible role and directory rather than by
/// the project compose. **Deploy-time meaning**: callers use this to drop the
/// service from the deploy payload and from remote-secret targets, so adding a
/// code here silently removes a user's service if they declare one by that name.
///
/// Kept to the two components that genuinely cannot be a user's own service.
/// Proxies are *not* here: Stacker's generator emits them itself and labels
/// them, and a user may legitimately run their own `caddy`. For deciding what
/// to *show* in the dashboard, use [`classify_scope`] instead — a wider and
/// safer question, because it is label-first.
const PLATFORM_MANAGED_APP_CODES: &[&str] = &["nginx_proxy_manager", "statuspanel"];

/// Codes that identify a platform component when a container carries no
/// `my.stacker.scope` label. **Display-time only** — never used to decide what
/// gets deployed.
///
/// Only components the platform installs itself. Proxies are absent on purpose:
/// the ones Stacker creates are labelled (`cli/generator/compose.rs`), and one a
/// user brought is theirs. Telegraf is absent too — it is monitoring
/// infrastructure by nature, but the user installs it by choice, so it is their
/// app and belongs in the Applications list.
///
/// See `config/docs/CONTAINER_APP_ARCHITECTURE.md` (Resolution, 2026-09-06) and
/// `config/shared-fixtures/agent-contract/app-code-resolution.md`.
const PLATFORM_DISPLAY_CODES: &[&str] =
    &["nginx_proxy_manager", "statuspanel", "statuspanel_agent"];

/// Directories platform-managed services are deployed into, by the convention
/// in `stacker/docs/APP_DEPLOYMENT.md`: project services live under the
/// project directory, platform ones get their own.
const PLATFORM_INSTALL_DIRS: &[&str] = &[
    "/home/trydirect/statuspanel",
    "/home/trydirect/nginx_proxy_manager",
];

/// Image names that identify a platform component when nothing better is
/// available. Compared against the image's last path segment without its tag.
const PLATFORM_IMAGE_NAMES: &[&str] = &["status", "nginx_proxy_manager"];

/// Who owns a container: the user's stack, or the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Project,
    Platform,
}

impl Scope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Scope::Project => crate::helpers::stacker_labels::SCOPE_PROJECT,
            Scope::Platform => crate::helpers::stacker_labels::SCOPE_PLATFORM,
        }
    }

    pub(crate) fn is_platform(self) -> bool {
        matches!(self, Scope::Platform)
    }
}

/// Decide whether a container belongs to the user's project or to the platform.
///
/// Signals in descending order of reliability:
///
/// 1. `my.stacker.scope` — authoritative in both directions. Stacker's compose
///    generator already sets it on every service it emits, `platform` for the
///    proxies (`cli/generator/compose.rs`);
/// 2. the Compose working directory, against the deployment-scope convention;
/// 3. the image name;
/// 4. an **exact** code match, after normalisation.
///
/// Anything unrecognised is `Project`. Guessing "platform" would hide a user's
/// own container from their Applications list, which is the worse mistake: a
/// misplaced platform container is untidy, a missing user app looks like data
/// loss.
///
/// Substring matching is deliberately absent. The agent used to classify by
/// `name.contains("status")`, which also catches a user's `status-page`.
pub(crate) fn classify_scope(
    labels: Option<&serde_json::Value>,
    app_code: Option<&str>,
    container_name: Option<&str>,
    image: Option<&str>,
) -> Scope {
    if let Some(scope) = labels.and_then(scope_label) {
        return scope;
    }

    if let Some(dir) = labels.and_then(|l| label_str(l, "com.docker.compose.project.working_dir")) {
        let dir = dir.trim_end_matches('/');
        if PLATFORM_INSTALL_DIRS.iter().any(|known| dir == *known) {
            return Scope::Platform;
        }
    }

    if let Some(image_name) = image.and_then(image_identity) {
        if PLATFORM_IMAGE_NAMES.contains(&image_name.as_str()) {
            return Scope::Platform;
        }
    }

    for candidate in [app_code, container_name].into_iter().flatten() {
        if PLATFORM_DISPLAY_CODES.contains(&normalize_app_code(candidate).as_str()) {
            return Scope::Platform;
        }
    }

    Scope::Project
}

/// Read `my.stacker.scope` from a container's label map, if present and known.
fn scope_label(labels: &serde_json::Value) -> Option<Scope> {
    match label_str(labels, crate::helpers::stacker_labels::SCOPE)?.as_str() {
        crate::helpers::stacker_labels::SCOPE_PLATFORM => Some(Scope::Platform),
        crate::helpers::stacker_labels::SCOPE_PROJECT => Some(Scope::Project),
        _ => None,
    }
}

fn label_str(labels: &serde_json::Value, key: &str) -> Option<String> {
    labels
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The image's last path segment without its tag, normalised.
fn image_identity(image: &str) -> Option<String> {
    let name = image.split('/').next_back()?.split(':').next()?;
    let normalized = normalize_app_code(name);
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn is_platform_managed_app_code(value: &str) -> bool {
    let normalized = normalize_app_code(value);
    PLATFORM_MANAGED_APP_CODES.contains(&normalized.as_str())
}

pub(crate) fn is_platform_managed_app_identity(service_name: &str, image: Option<&str>) -> bool {
    app_identity_candidates(service_name, image)
        .iter()
        .any(|candidate| is_platform_managed_app_code(candidate))
}

pub(crate) fn is_nginx_proxy_manager_identity(service_name: &str, image: Option<&str>) -> bool {
    app_identity_candidates(service_name, image)
        .iter()
        .any(|candidate| candidate == "nginx_proxy_manager")
}

/// Canonical form of an app code: lowercase, separators collapsed to `_`.
///
/// Spaces and dots count as separators alongside `-` and `_`. Without them a
/// display name like "Status Panel" normalised to `status panel`, which matched
/// nothing, so the Status Panel appeared in the user's Applications list.
pub(crate) fn normalize_app_code(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('/')
        .to_lowercase()
        .split(['-', '_', ' ', '.'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<&str>>()
        .join("_")
}

fn app_identity_candidates(service_name: &str, image: Option<&str>) -> Vec<String> {
    let normalized_service_name = normalize_app_code(service_name);
    let mut candidates = vec![normalized_service_name.clone()];
    if normalized_service_name == "npm" {
        candidates.push("nginx_proxy_manager".to_string());
    }

    if let Some(image) = image {
        if let Some(image_name) = image.split('/').last() {
            if let Some(name_without_tag) = image_name.split(':').next() {
                let normalized_image_name = normalize_app_code(name_without_tag);
                if normalized_image_name == "npm" {
                    candidates.push("nginx_proxy_manager".to_string());
                }
                candidates.push(normalized_image_name);
            }
        }
    }

    candidates
}

pub(crate) fn is_compose_filename(file_name: &str) -> bool {
    matches!(
        file_name,
        "compose"
            | "compose.yml"
            | "compose.yaml"
            | "docker-compose"
            | "docker-compose.yml"
            | "docker-compose.yaml"
    )
}

#[cfg(test)]
mod tests;
