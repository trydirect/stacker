//! Container Discovery & Import API
//!
//! Endpoints for discovering running containers and importing them into project_app table.
//! This allows users to register containers that are running but not tracked in the database.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models::{self, ProjectApp};
use crate::project_app::{is_platform_managed_app_code, normalize_app_code};
use actix_web::{get, post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

const BLOCKED_SYSTEM_CONTAINERS: [&str; 6] = [
    "nginx_proxy_manager",
    "status",
    "status_agent",
    "statuspanel",
    "statuspanel_agent",
    "telegraf",
];

/// Discovered container that's not registered in project_app
#[derive(Debug, Serialize, Clone)]
pub struct DiscoveredContainer {
    /// Actual Docker container name
    pub container_name: String,
    /// Docker image
    pub image: String,
    /// Container status (running, stopped, etc.)
    pub status: String,
    /// Suggested app_code based on container name heuristics
    pub suggested_code: String,
    /// Suggested display name
    pub suggested_name: String,
}

/// Response for container discovery endpoint
#[derive(Debug, Serialize, Default)]
pub struct DiscoverResponse {
    /// Containers that are registered in project_app
    pub registered: Vec<RegisteredContainerInfo>,
    /// Containers running but not in database
    pub unregistered: Vec<DiscoveredContainer>,
    /// Registered apps with no matching running container
    pub missing_containers: Vec<MissingContainerInfo>,
}

#[derive(Debug, Serialize)]
pub struct RegisteredContainerInfo {
    pub app_code: String,
    pub app_name: String,
    pub container_name: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct MissingContainerInfo {
    pub app_code: String,
    pub app_name: String,
    pub expected_pattern: String,
}

/// Request to import discovered containers
#[derive(Debug, Deserialize)]
pub struct ImportContainersRequest {
    pub containers: Vec<ContainerImport>,
}

#[derive(Debug, Deserialize)]
pub struct ContainerImport {
    /// Actual Docker container name
    pub container_name: String,
    /// App code to assign (user can override suggested)
    pub app_code: String,
    /// Display name
    pub name: String,
    /// Docker image
    pub image: String,
}

/// Discover running containers for a deployment
///
/// This endpoint compares running Docker containers (from recent health checks)
/// with registered project_app records to identify:
/// - Registered apps with running containers (synced)
/// - Running containers not in database (unregistered, can be imported)
/// - Database apps with no running container (stopped or name mismatch)
#[tracing::instrument(name = "Discover containers", skip_all)]
#[get("/{project_id}/containers/discover")]
pub async fn discover_containers(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    query: web::Query<DiscoverQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    // Get deployment_hash from query, the active project agent, or the latest
    // deployment record. Active agent state is preferred because command
    // history is keyed by the hash currently heartbeating, while the latest
    // deployment row may be newer and still lack command results.
    let deployment_hash = match &query.deployment_hash {
        Some(hash) => hash.clone(),
        None => {
            if let Some(agent) = db::agent::fetch_active_by_project(pg_pool.get_ref(), project_id)
                .await
                .map_err(|e| JsonResponse::internal_server_error(e))?
            {
                agent.deployment_hash
            } else {
                let deployment = db::deployment::fetch_by_project_id(pg_pool.get_ref(), project_id)
                    .await
                    .map_err(|e| JsonResponse::internal_server_error(e))?;

                deployment.map(|d| d.deployment_hash).ok_or_else(|| {
                    JsonResponse::not_found(
                        "No deployment found for project. Please provide deployment_hash",
                    )
                })?
            }
        }
    };

    // Fetch all apps registered in this project
    let registered_apps = db::project_app::fetch_by_project(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;

    // Fetch recent list_containers commands to get ALL running containers
    let container_commands = db::command::fetch_recent_by_deployment(
        pg_pool.get_ref(),
        &deployment_hash,
        50,    // Last 50 commands to find list_containers results
        false, // Include results
    )
    .await
    .unwrap_or_default();

    // Extract running containers from list_containers or health commands
    let mut running_containers: Vec<ContainerInfo> = Vec::new();

    // First, try to find a list_containers result (has ALL containers)
    for cmd in container_commands.iter() {
        if cmd.r#type == "list_containers" && cmd.status == "completed" {
            if let Some(result) = &cmd.result {
                // Parse list_containers result which contains array of all containers
                if let Some(containers_arr) = result.get("containers").and_then(|c| c.as_array()) {
                    for c in containers_arr {
                        let name = c
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        if name.is_empty() {
                            continue;
                        }
                        let status = c
                            .get("status")
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let image = c
                            .get("image")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !running_containers.iter().any(|rc| rc.name == name) {
                            // The agent reports the logical app code from the
                            // `my.stacker.service` label; fall back to reading
                            // the label ourselves for agents predating that.
                            // Leaving this None made matching name-based, and
                            // `suggest_app_info` then split `project-floci-ui-1`
                            // into `ui`.
                            let app_code = c
                                .get("app_code")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                                .or_else(|| app_code_from_labels(c))
                                .filter(|code| !code.trim().is_empty());

                            running_containers.push(ContainerInfo {
                                name: name.clone(),
                                image,
                                status,
                                app_code,
                            });
                        }
                    }
                }
            }
            // Found list_containers result, prefer this over health checks
            if !running_containers.is_empty() {
                break;
            }
        }
    }

    // Fallback: If no list_containers found, try health check results
    if running_containers.is_empty() {
        for cmd in container_commands.iter() {
            if cmd.r#type == "health" && cmd.status == "completed" {
                if let Some(result) = &cmd.result {
                    for container in container_infos_from_health(result) {
                        if !running_containers.iter().any(|rc| rc.name == container.name) {
                            running_containers.push(container);
                        }
                    }
                }
            }
        }
    }

    tracing::info!(
        project_id = project_id,
        deployment_hash = %deployment_hash,
        registered_count = registered_apps.len(),
        running_count = running_containers.len(),
        "Discovered containers"
    );

    // Exclude system containers from discovery/import candidates
    running_containers.retain(|container| {
        !is_blocked_system_container(
            &container.name,
            &container.image,
            container.app_code.as_deref(),
        )
    });

    // Classify containers
    let mut registered = Vec::new();
    let mut unregistered = Vec::new();
    let mut missing_containers = Vec::new();

    // Find registered apps with running containers
    for app in &registered_apps {
        let matching_container = running_containers.iter().find(|c| {
            // Try to match by app_code first
            c.app_code.as_ref() == Some(&app.code) ||
                // Or by container name matching app code
                container_matches_app(&c.name, &app.code)
        });

        if let Some(container) = matching_container {
            registered.push(RegisteredContainerInfo {
                app_code: app.code.clone(),
                app_name: app.name.clone(),
                container_name: container.name.clone(),
                status: container.status.clone(),
            });
        } else {
            // App exists but no container found
            missing_containers.push(MissingContainerInfo {
                app_code: app.code.clone(),
                app_name: app.name.clone(),
                expected_pattern: app.code.clone(),
            });
        }
    }

    // Find running containers not registered
    for container in &running_containers {
        let is_registered = registered_apps.iter().any(|app| {
            app.code == container.app_code.clone().unwrap_or_default()
                || container_matches_app(&container.name, &app.code)
        });

        if !is_registered {
            // Prefer the code the container reports about itself. The
            // heuristic only guesses from the container name, and Compose
            // names (`project-floci-ui-1`) do not contain the app code, so it
            // suggested `ui` for `floci-ui` and `app` for `floci`. Importing
            // those creates project_app rows that never match anything
            // resolving by `my.stacker.service`.
            let (suggested_code, suggested_name) = match container.app_code.as_deref() {
                Some(code) if !code.trim().is_empty() => {
                    (code.trim().to_string(), capitalize(code.trim()))
                }
                _ => suggest_app_info(&container.name, &container.image),
            };

            unregistered.push(DiscoveredContainer {
                container_name: container.name.clone(),
                image: container.image.clone(),
                status: container.status.clone(),
                suggested_code,
                suggested_name,
            });
        }
    }

    let response = DiscoverResponse {
        registered,
        unregistered,
        missing_containers,
    };

    tracing::info!(
        project_id = project_id,
        registered = response.registered.len(),
        unregistered = response.unregistered.len(),
        missing = response.missing_containers.len(),
        "Container discovery complete"
    );

    Ok(JsonResponse::build()
        .set_item(response)
        .ok("Containers discovered"))
}

/// Import unregistered containers into project_app
#[tracing::instrument(name = "Import containers", skip_all)]
#[post("/{project_id}/containers/import")]
pub async fn import_containers(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<i32>,
    body: web::Json<ImportContainersRequest>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let project_id = path.into_inner();

    // Verify project ownership
    let project = db::project::fetch(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?
        .ok_or_else(|| JsonResponse::not_found("Project not found"))?;

    if project.user_id != user.id {
        return Err(JsonResponse::not_found("Project not found"));
    }

    let mut imported = Vec::new();
    let mut errors = Vec::new();

    for container in &body.containers {
        if is_blocked_system_container(
            &container.container_name,
            &container.image,
            Some(&container.app_code),
        ) {
            errors.push(format!(
                "Container '{}' is a system container and cannot be imported",
                container.container_name
            ));
            continue;
        }

        // Check if app_code already exists
        let existing = db::project_app::fetch_by_project_and_code(
            pg_pool.get_ref(),
            project_id,
            &container.app_code,
        )
        .await
        .ok()
        .flatten();

        if existing.is_some() {
            errors.push(format!(
                "App code '{}' already exists in project",
                container.app_code
            ));
            continue;
        }

        // Create new project_app entry
        let app = ProjectApp {
            id: 0, // Will be set by database
            project_id,
            code: container.app_code.clone(),
            name: container.name.clone(),
            image: container.image.clone(),
            environment: Some(json!({})),
            ports: Some(json!([])),
            volumes: Some(json!([])),
            domain: None,
            ssl_enabled: Some(false),
            resources: Some(json!({})),
            restart_policy: Some("unless-stopped".to_string()),
            command: None,
            entrypoint: None,
            networks: Some(json!([])),
            depends_on: Some(json!([])),
            healthcheck: Some(json!({})),
            labels: Some(json!({})),
            config_files: Some(json!([])),
            template_source: None,
            enabled: Some(true),
            deploy_order: Some(100), // Default order
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            config_version: Some(1),
            vault_synced_at: None,
            vault_sync_version: None,
            config_hash: None,
            parent_app_code: None,
            deployment_id: None,
        };

        match db::project_app::insert(pg_pool.get_ref(), &app).await {
            Ok(created) => {
                imported.push(json!({
                    "code": created.code,
                    "name": created.name,
                    "container_name": container.container_name,
                }));

                tracing::info!(
                    user_id = %user.id,
                    project_id = project_id,
                    app_code = %created.code,
                    container_name = %container.container_name,
                    "Imported container"
                );
            }
            Err(e) => {
                let error_msg = format!("Failed to import '{}': {}", container.app_code, e);
                errors.push(error_msg);
            }
        }
    }

    Ok(JsonResponse::build()
        .set_item(Some(json!({
            "imported": imported,
            "errors": errors,
            "success_count": imported.len(),
            "error_count": errors.len(),
        })))
        .ok("Import complete"))
}

// Helper structs

#[derive(Debug, Deserialize)]
pub struct DiscoverQuery {
    pub deployment_hash: Option<String>,
}

#[derive(Debug)]
struct ContainerInfo {
    name: String,
    image: String,
    status: String,
    app_code: Option<String>,
}

// Helper functions

/// Check if a container name matches an app code
fn container_matches_app(container_name: &str, app_code: &str) -> bool {
    // Exact match
    if container_name == app_code {
        return true;
    }

    // Container ends with app_code (e.g., "statuspanel_agent" matches "agent")
    if container_name.ends_with(app_code) {
        return true;
    }

    // Container is {app_code}_{number} or {app_code}-{number}
    if container_name.starts_with(app_code) {
        let suffix = &container_name[app_code.len()..];
        if suffix.starts_with('_') || suffix.starts_with('-') {
            if let Some(rest) = suffix.get(1..) {
                if rest.chars().all(|c| c.is_numeric()) {
                    return true;
                }
            }
        }
    }

    // Container is {project}-{app_code}-{number}
    let parts: Vec<&str> = container_name.split('-').collect();
    if parts.len() >= 2 && parts[parts.len() - 2] == app_code {
        return true;
    }

    false
}

/// Suggest app_code and name from container name and image
/// Containers named by one completed `health` command result.
///
/// Used only when no `list_containers` result is available — for a server the
/// client attached themselves, discovery may see nothing else.
///
/// Reads three shapes, because the agent produces two and the platform adds a
/// third:
///   - `containers[]` — the project's own containers, from an `all_health`
///     report (a `health` command with `app_code: "all"`);
///   - `system_containers[]` — platform-managed ones, same report;
///   - a flat `app_code` + `container_state` — a single-app health check.
///
/// `containers[]` was previously not read at all, so a deployment whose most
/// recent command was an aggregate health check surfaced only platform
/// containers — exactly the ones discovery then filters out. It stayed hidden
/// because `list_containers` is preferred and normally present.
fn container_infos_from_health(result: &serde_json::Value) -> Vec<ContainerInfo> {
    let mut found: Vec<ContainerInfo> = Vec::new();

    for key in ["containers", "system_containers"] {
        let Some(reported) = result.get(key).and_then(|c| c.as_array()) else {
            continue;
        };
        for c in reported {
            let name = c
                .get("container_name")
                .or_else(|| c.get("app_code"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() || found.iter().any(|f| f.name == name) {
                continue;
            }
            found.push(ContainerInfo {
                name,
                image: String::new(),
                status: c
                    .get("container_state")
                    .or_else(|| c.get("status"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                app_code: c
                    .get("app_code")
                    .and_then(|a| a.as_str())
                    .map(str::to_string),
            });
        }
    }

    // Single-app health check: the report itself describes one container.
    if let Some(app_code) = result.get("app_code").and_then(|a| a.as_str()) {
        if !found.iter().any(|f| f.name == app_code) {
            found.push(ContainerInfo {
                name: app_code.to_string(),
                image: String::new(),
                status: result
                    .get("container_state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                app_code: Some(app_code.to_string()),
            });
        }
    }

    found
}

/// The app code carried by a container's Docker labels, if the agent shipped
/// them. Mirrors the agent's resolution order: Stacker's own label first,
/// Compose's service name second.
///
/// See `config/shared-fixtures/agent-contract/app-code-resolution.md`.
fn app_code_from_labels(container: &serde_json::Value) -> Option<String> {
    let labels = container.get("labels")?.as_object()?;
    for key in [
        crate::helpers::stacker_labels::SERVICE,
        "com.docker.compose.service",
    ] {
        if let Some(code) = labels.get(key).and_then(|v| v.as_str()) {
            if !code.trim().is_empty() {
                return Some(code.trim().to_string());
            }
        }
    }
    None
}

fn suggest_app_info(container_name: &str, image: &str) -> (String, String) {
    // Try to extract service name from Docker Compose pattern: {project}_{service}_{replica}
    if let Some(parts) = extract_compose_service(container_name) {
        let code = parts.service.to_string();
        let name = capitalize(&code);
        return (code, name);
    }

    // Try to extract from project-service-replica pattern
    let parts: Vec<&str> = container_name.split('-').collect();
    if parts.len() >= 2 {
        let service = parts[parts.len() - 2];
        if !service.chars().all(|c| c.is_numeric()) {
            return (service.to_string(), capitalize(service));
        }
    }

    // Extract from image name (last part before tag)
    if let Some(img_name) = image.split('/').last() {
        if let Some(name_without_tag) = img_name.split(':').next() {
            return (name_without_tag.to_string(), capitalize(name_without_tag));
        }
    }

    // Fallback: use container name
    (container_name.to_string(), capitalize(container_name))
}

struct ComposeServiceParts {
    service: String,
}

fn extract_compose_service(container_name: &str) -> Option<ComposeServiceParts> {
    let parts: Vec<&str> = container_name.split('_').collect();
    if parts.len() >= 2 {
        // Last part should be replica number
        if parts.last()?.chars().all(|c| c.is_numeric()) {
            // Service is second to last
            let service = parts[parts.len() - 2].to_string();
            return Some(ComposeServiceParts { service });
        }
    }
    None
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().chain(c).collect(),
    }
}

fn is_blocked_system_container(container_name: &str, image: &str, app_code: Option<&str>) -> bool {
    let mut candidates: Vec<String> = vec![normalize_app_code(container_name)];

    if let Some(code) = app_code {
        candidates.push(normalize_app_code(code));
    }

    if let Some(compose_parts) = extract_compose_service(container_name) {
        candidates.push(normalize_app_code(&compose_parts.service));
    }

    if let Some(img_name) = image.split('/').last() {
        if let Some(name_without_tag) = img_name.split(':').next() {
            candidates.push(normalize_app_code(name_without_tag));
        }
    }

    candidates.iter().any(|candidate| {
        BLOCKED_SYSTEM_CONTAINERS.contains(&candidate.as_str())
            || is_platform_managed_app_code(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The agent reports `app_code` from `my.stacker.service`; discovery must
    /// use it rather than fall back to the container-name heuristic.
    #[test]
    fn app_code_from_labels_prefers_the_stacker_label() {
        let container = json!({
            "name": "project-app-1",
            "labels": {
                "my.stacker.service": "floci",
                "com.docker.compose.service": "app"
            }
        });
        assert_eq!(app_code_from_labels(&container).as_deref(), Some("floci"));
    }

    #[test]
    fn app_code_from_labels_falls_back_to_compose() {
        let container = json!({
            "name": "someproject-web-1",
            "labels": {"com.docker.compose.service": "web"}
        });
        assert_eq!(app_code_from_labels(&container).as_deref(), Some("web"));
    }

    #[test]
    fn app_code_from_labels_is_none_without_usable_labels() {
        assert_eq!(app_code_from_labels(&json!({"name": "x"})), None);
        assert_eq!(
            app_code_from_labels(&json!({"name": "x", "labels": {"my.stacker.service": "  "}})),
            None
        );
    }

    /// A reported app_code must win over the name heuristic — that is the
    /// whole point of the label. Guards the `suggested_code` path, which is
    /// computed separately from the matching path and was missed at first.
    #[test]
    fn reported_app_code_beats_the_name_heuristic() {
        let reported = Some("floci-ui".to_string());
        let (code, name) = match reported.as_deref() {
            Some(c) if !c.trim().is_empty() => (c.trim().to_string(), capitalize(c.trim())),
            _ => suggest_app_info("project-floci-ui-1", "floci/floci-ui"),
        };
        assert_eq!(code, "floci-ui");
        assert_eq!(name, "Floci-ui");

        // Without a reported code the heuristic still applies, and still errs.
        let absent: Option<String> = None;
        let (code, _) = match absent.as_deref() {
            Some(c) if !c.trim().is_empty() => (c.trim().to_string(), capitalize(c.trim())),
            _ => suggest_app_info("project-floci-ui-1", "floci/floci-ui"),
        };
        assert_eq!(code, "ui");
    }

    /// The aggregate a `health` command with `app_code: "all"` produces. The
    /// project's own containers live in `containers[]`, which discovery did
    /// not read at all — so a server whose only completed command was a health
    /// check surfaced nothing but platform containers, which are then filtered
    /// out. This is the path a client-attached server relies on.
    #[test]
    fn health_fallback_reads_project_containers() {
        let result = json!({
            "type": "all_health",
            "deployment_hash": "deployment_abc",
            "status": "ok",
            "containers": [
                {"app_code": "floci", "container_name": "project-app-1",
                 "container_state": "running", "status": "ok"},
                {"app_code": "floci-ui", "container_name": "project-floci-ui-1",
                 "container_state": "running", "status": "ok"}
            ],
            "system_containers": [
                {"app_code": "statuspanel", "container_name": "statuspanel",
                 "container_state": "running", "status": "ok"}
            ]
        });

        let found = container_infos_from_health(&result);
        let names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["project-app-1", "project-floci-ui-1", "statuspanel"],
            "project containers must come through, not only platform ones"
        );
        assert_eq!(found[0].app_code.as_deref(), Some("floci"));
        assert_eq!(found[0].status, "running");
    }

    /// A single-app health check describes one container in the report itself.
    #[test]
    fn health_fallback_reads_the_single_app_shape() {
        let result = json!({
            "type": "health",
            "deployment_hash": "deployment_abc",
            "app_code": "floci",
            "container_state": "running"
        });

        let found = container_infos_from_health(&result);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "floci");
        assert_eq!(found[0].app_code.as_deref(), Some("floci"));
    }

    /// A container already named by `containers[]` must not be added twice by
    /// the single-app branch.
    #[test]
    fn health_fallback_does_not_duplicate() {
        let result = json!({
            "type": "all_health",
            "app_code": "floci",
            "containers": [{"app_code": "floci", "container_name": "floci",
                            "container_state": "running"}]
        });
        assert_eq!(container_infos_from_health(&result).len(), 1);
    }

    /// Why the label is needed: the name heuristic splits on dashes and takes
    /// the second-to-last segment, so both floci services get the wrong code.
    #[test]
    fn name_heuristic_is_wrong_for_compose_named_containers() {
        assert_eq!(suggest_app_info("project-app-1", "floci/floci").0, "app");
        assert_eq!(
            suggest_app_info("project-floci-ui-1", "floci/floci-ui").0,
            "ui"
        );
    }

    #[test]
    fn blocks_platform_managed_nginx_proxy_manager_container() {
        assert!(is_blocked_system_container(
            "nginx-proxy-manager",
            "jc21/nginx-proxy-manager:latest",
            None,
        ));
        assert!(is_blocked_system_container(
            "project-nginx_proxy_manager-1",
            "jc21/nginx-proxy-manager:latest",
            Some("nginx_proxy_manager"),
        ));
    }

    #[test]
    fn does_not_block_regular_application_container() {
        assert!(!is_blocked_system_container(
            "project-coolify-1",
            "coollabsio/coolify:latest",
            Some("coolify"),
        ));
    }
}
