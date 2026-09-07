use crate::db;
use crate::forms::status_panel::{AllHealthCommandReport, HealthCommandReport};
use crate::helpers::{AgentPgPool, JsonResponse};
use crate::models::{Command, ProjectApp};
use crate::project_app::is_platform_managed_app_code;
use actix_web::{get, web, Responder, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Default)]
pub struct SnapshotResponse {
    /// Stacker's numeric project id for this deployment.
    ///
    /// The dashboard reaches this endpoint by `deployment_hash` and has no
    /// other way to learn it: the deployment record it renders from carries
    /// `stack_id`, a UUID, while every project-scoped endpoint
    /// (`/agent/project/{id}`, `/project/{id}/containers/discover`) keys on
    /// this integer. Without it the UI cannot ask whether an agent is
    /// connected, and falls back to guessing from the install request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i32>,
    pub agent: Option<AgentSnapshot>,
    pub commands: Vec<Command>,
    pub containers: Vec<ContainerSnapshot>,
    pub apps: Vec<ProjectApp>,
}

#[derive(Debug, Serialize, Default)]
pub struct AgentSnapshot {
    pub id: Option<Uuid>,
    pub version: Option<String>,
    pub capabilities: Option<serde_json::Value>,
    pub system_info: Option<serde_json::Value>,
    pub status: Option<String>,
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
    pub deployment_hash: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct ContainerSnapshot {
    pub id: Option<String>,
    pub app: Option<String>,
    pub state: Option<String>,
    pub image: Option<String>,
    pub name: Option<String>,
}

/// Container states from one completed `health` command result.
///
/// Both report shapes must be read here. A `health` command carrying
/// `app_code: "all"` — which is what the dashboard and the CLI send — is
/// answered with the aggregate `all_health`, whose per-container `app_code`
/// and `container_state` live inside `containers[]`. A command naming one app
/// is answered with the flat single-app shape.
///
/// Parsing only the single shape left `containers` empty for every deployment
/// polled with `all`, so the UI reported "Status Panel has not reported
/// containers yet" while four containers were running and reporting. The old
/// code also swallowed the parse failure with a bare `if let Ok(..)`, which is
/// why nothing in the logs pointed at it.
fn container_snapshots_from_health(result: &serde_json::Value) -> Vec<ContainerSnapshot> {
    fn state_of<T: serde::Serialize>(container_state: &T) -> Option<String> {
        serde_json::to_value(container_state)
            .ok()
            .and_then(|v| v.as_str().map(str::to_lowercase))
    }

    // Dispatch on the reported type, never on "which struct happens to
    // deserialize": the aggregate's `containers` is `#[serde(default)]`, so
    // `AllHealthCommandReport` also parses a single-app report and silently
    // yields an empty list. `validate_command_result` dispatches the same way.
    let is_aggregate = result.get("type").and_then(|v| v.as_str())
        == Some(crate::forms::status_panel::ALL_HEALTH_RESULT_TYPE);

    if is_aggregate {
        if let Ok(all) = serde_json::from_value::<AllHealthCommandReport>(result.clone()) {
            return all
                .containers
                .iter()
                .chain(all.system_containers.iter())
                .map(|c| ContainerSnapshot {
                    id: None,
                    app: Some(c.app_code.clone()),
                    state: state_of(&c.container_state),
                    image: None,
                    name: c.container_name.clone(),
                })
                .collect();
        }
    }

    if let Ok(single) = serde_json::from_value::<HealthCommandReport>(result.clone()) {
        return vec![ContainerSnapshot {
            id: None,
            app: Some(single.app_code.clone()),
            state: state_of(&single.container_state),
            image: None,
            name: None,
        }];
    }

    tracing::debug!(
        "health result matched neither the aggregate nor the single-app shape; ignoring"
    );
    Vec::new()
}

#[derive(Debug, Deserialize)]
pub struct SnapshotQuery {
    #[serde(default = "default_command_limit")]
    pub command_limit: i64,
    #[serde(default)]
    pub include_command_results: bool,
}

fn default_command_limit() -> i64 {
    50
}

fn visible_project_apps(apps: Vec<ProjectApp>) -> Vec<ProjectApp> {
    apps.into_iter()
        .filter(|app| !is_platform_managed_app_code(&app.code))
        .collect()
}

#[tracing::instrument(name = "Get deployment snapshot", skip_all)]
#[get("/deployments/{deployment_hash}")]
pub async fn snapshot_handler(
    path: web::Path<String>,
    query: web::Query<SnapshotQuery>,
    agent_pool: web::Data<AgentPgPool>,
    settings: web::Data<crate::configuration::Settings>,
    caller_agent: Option<web::ReqData<std::sync::Arc<crate::models::Agent>>>,
    caller_user: Option<web::ReqData<std::sync::Arc<crate::models::User>>>,
) -> Result<impl Responder> {
    tracing::info!(
        "[SNAPSHOT HANDLER] Called for deployment_hash: {}, limit: {}, include_results: {}",
        path,
        query.command_limit,
        query.include_command_results
    );
    let deployment_hash = path.into_inner();

    // Casbin only matches `/api/v1/agent/deployments/*`, so the caller must be
    // checked against *this* deployment before anything is read.
    crate::routes::agent::guard::authorize_deployment_access(
        agent_pool.get_ref(),
        settings.get_ref(),
        &deployment_hash,
        caller_agent.as_deref(),
        caller_user.as_deref(),
    )
    .await?;

    // Fetch agent
    let agent = db::agent::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
        .await
        .ok()
        .flatten();

    tracing::debug!("[SNAPSHOT HANDLER] Agent : {:?}", agent);
    // Fetch recent commands with optional result exclusion to reduce payload size
    let commands = db::command::fetch_recent_by_deployment(
        agent_pool.get_ref(),
        &deployment_hash,
        query.command_limit,
        !query.include_command_results,
    )
    .await
    .unwrap_or_default();

    tracing::debug!("[SNAPSHOT HANDLER] Commands : {:?}", commands);
    // Fetch deployment to get project_id
    let deployment =
        db::deployment::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
            .await
            .ok()
            .flatten();

    tracing::debug!("[SNAPSHOT HANDLER] Deployment : {:?}", deployment);
    // Fetch apps scoped to this specific deployment (falls back to project-level if no deployment-scoped apps)
    let apps = if let Some(deployment) = &deployment {
        db::project_app::fetch_by_deployment(
            agent_pool.get_ref(),
            deployment.project_id,
            deployment.id,
        )
        .await
        .unwrap_or_default()
    } else {
        vec![]
    };
    let apps = visible_project_apps(apps);

    tracing::debug!("[SNAPSHOT HANDLER] Apps : {:?}", apps);

    // Fetch recent health commands WITH results to populate container states
    // (we always need health results for container status, even if include_command_results=false)
    let health_commands = db::command::fetch_recent_by_deployment(
        agent_pool.get_ref(),
        &deployment_hash,
        10,    // Fetch last 10 health checks
        false, // Always include results for health commands
    )
    .await
    .unwrap_or_default();

    // Extract container states from recent health check commands
    // Use a HashMap to keep only the most recent health check per app_code
    let mut container_map: std::collections::HashMap<String, ContainerSnapshot> =
        std::collections::HashMap::new();

    for cmd in health_commands.iter() {
        if cmd.r#type == "health" && cmd.status == "completed" {
            if let Some(result) = &cmd.result {
                for container in container_snapshots_from_health(result) {
                    let Some(app_code) = container.app.clone() else {
                        continue;
                    };
                    // Keep the most recent report per app (commands arrive DESC).
                    container_map.entry(app_code).or_insert(container);
                }
            }
        }
    }

    let containers: Vec<ContainerSnapshot> = container_map.into_values().collect();

    tracing::debug!(
        "[SNAPSHOT HANDLER] Containers extracted from {} health checks: {:?}",
        health_commands.len(),
        containers
    );

    // Derive effective status: if heartbeat is stale (>5 min), override to "offline"
    let agent_snapshot = agent.map(|a| {
        let effective_status = match a.last_heartbeat {
            Some(hb) => {
                let stale_threshold = chrono::Duration::seconds(300); // 5 minutes
                if chrono::Utc::now() - hb > stale_threshold {
                    "offline".to_string()
                } else {
                    a.status.clone()
                }
            }
            None => "offline".to_string(), // Never had a heartbeat
        };
        AgentSnapshot {
            id: Some(a.id),
            version: a.version,
            capabilities: a.capabilities,
            system_info: a.system_info,
            status: Some(effective_status),
            last_heartbeat: a.last_heartbeat,
            deployment_hash: Some(a.deployment_hash),
        }
    });
    tracing::debug!("[SNAPSHOT HANDLER] Agent Snapshot : {:?}", agent_snapshot);

    let resp = SnapshotResponse {
        project_id: deployment.as_ref().map(|d| d.project_id),
        agent: agent_snapshot,
        commands,
        containers,
        apps,
    };

    tracing::info!("[SNAPSHOT HANDLER] Snapshot response prepared: {:?}", resp);
    Ok(JsonResponse::build()
        .set_item(resp)
        .ok("Snapshot fetched successfully"))
}

/// Returns the snapshot for the most recently active agent in a project.
/// Used by the CLI as a stable project-scoped alternative to deployment-hash lookup.
#[tracing::instrument(name = "Get project agent snapshot", skip_all)]
#[get("/project/{project_id}")]
pub async fn project_snapshot_handler(
    path: web::Path<i32>,
    agent_pool: web::Data<AgentPgPool>,
    caller_agent: Option<web::ReqData<std::sync::Arc<crate::models::Agent>>>,
    caller_user: Option<web::ReqData<std::sync::Arc<crate::models::User>>>,
) -> Result<impl Responder> {
    let project_id = path.into_inner();

    crate::routes::agent::guard::authorize_project_access(
        agent_pool.get_ref(),
        project_id,
        caller_agent.as_deref(),
        caller_user.as_deref(),
    )
    .await?;

    let agent = db::agent::fetch_active_by_project(agent_pool.get_ref(), project_id)
        .await
        .ok()
        .flatten();

    let agent_snapshot = match agent {
        None => {
            // Still echo the project id: the caller asked by project and the
            // field must not appear only on the happy path.
            return Ok(JsonResponse::build()
                .set_item(SnapshotResponse {
                    project_id: Some(project_id),
                    ..Default::default()
                })
                .ok("No active agent found for project"));
        }
        Some(a) => {
            let effective_status = match a.last_heartbeat {
                Some(hb) => {
                    let stale_threshold = chrono::Duration::seconds(300);
                    if chrono::Utc::now() - hb > stale_threshold {
                        "offline".to_string()
                    } else {
                        a.status.clone()
                    }
                }
                None => "offline".to_string(),
            };
            let deployment_hash = a.deployment_hash.clone();

            let snap = AgentSnapshot {
                id: Some(a.id),
                version: a.version,
                capabilities: a.capabilities,
                system_info: a.system_info,
                status: Some(effective_status),
                last_heartbeat: a.last_heartbeat,
                deployment_hash: Some(deployment_hash.clone()),
            };
            (snap, deployment_hash)
        }
    };

    let (agent_snap, deployment_hash) = agent_snapshot;

    let commands =
        db::command::fetch_recent_by_deployment(agent_pool.get_ref(), &deployment_hash, 50, true)
            .await
            .unwrap_or_default();

    let deployment =
        db::deployment::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
            .await
            .ok()
            .flatten();

    let apps = if let Some(dep) = &deployment {
        db::project_app::fetch_by_deployment(agent_pool.get_ref(), dep.project_id, dep.id)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };
    let apps = visible_project_apps(apps);

    let health_commands =
        db::command::fetch_recent_by_deployment(agent_pool.get_ref(), &deployment_hash, 10, false)
            .await
            .unwrap_or_default();

    let mut container_map: std::collections::HashMap<String, ContainerSnapshot> =
        std::collections::HashMap::new();

    for cmd in health_commands.iter() {
        if cmd.r#type == "health" && cmd.status == "completed" {
            if let Some(result) = &cmd.result {
                for container in container_snapshots_from_health(result) {
                    let Some(app_code) = container.app.clone() else {
                        continue;
                    };
                    // Keep the most recent report per app (commands arrive DESC).
                    container_map.entry(app_code).or_insert(container);
                }
            }
        }
    }

    let containers: Vec<ContainerSnapshot> = container_map.into_values().collect();

    let resp = SnapshotResponse {
        project_id: Some(project_id),
        agent: Some(agent_snap),
        commands,
        containers,
        apps,
    };

    Ok(JsonResponse::build()
        .set_item(resp)
        .ok("Snapshot fetched successfully"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The aggregate the agent actually sends for `app_code: "all"`. Parsing
    /// only the single-app shape left this empty, and the dashboard showed
    /// "Status Panel has not reported containers yet" for a deployment with
    /// four running containers.
    #[test]
    fn health_snapshots_read_the_aggregate_shape() {
        let result = json!({
            "type": "all_health",
            "deployment_hash": "deployment_abc",
            "status": "ok",
            "containers": [
                {"app_code": "floci", "container_name": "project-app-1",
                 "container_state": "running", "status": "ok"},
                {"app_code": "floci-ui", "container_name": "project-floci-ui-1",
                 "container_state": "running", "status": "ok"}
            ]
        });

        let snaps = container_snapshots_from_health(&result);
        assert_eq!(
            snaps.len(),
            2,
            "both containers must be reported: {snaps:?}"
        );

        let codes: Vec<&str> = snaps.iter().filter_map(|c| c.app.as_deref()).collect();
        assert_eq!(codes, vec!["floci", "floci-ui"]);
        assert_eq!(snaps[0].state.as_deref(), Some("running"));
        assert_eq!(snaps[0].name.as_deref(), Some("project-app-1"));
    }

    /// A command naming one app still answers with the flat shape.
    #[test]
    fn health_snapshots_read_the_single_app_shape() {
        let result = json!({
            "type": "health",
            "deployment_hash": "deployment_abc",
            "app_code": "floci",
            "status": "ok",
            "container_state": "running"
        });

        let snaps = container_snapshots_from_health(&result);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].app.as_deref(), Some("floci"));
        assert_eq!(snaps[0].state.as_deref(), Some("running"));
    }

    /// Platform containers reported separately must still surface.
    #[test]
    fn health_snapshots_include_system_containers() {
        let result = json!({
            "type": "all_health",
            "deployment_hash": "deployment_abc",
            "status": "ok",
            "containers": [],
            "system_containers": [
                {"app_code": "statuspanel", "container_name": "statuspanel",
                 "container_state": "running", "status": "ok"}
            ]
        });

        let snaps = container_snapshots_from_health(&result);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].app.as_deref(), Some("statuspanel"));
    }

    /// Neither shape: yield nothing rather than panicking or inventing a row.
    #[test]
    fn health_snapshots_ignore_an_unrecognised_result() {
        assert!(container_snapshots_from_health(&json!({"nonsense": true})).is_empty());
    }

    /// The dashboard reaches this endpoint by `deployment_hash` and needs the
    /// numeric project id to ask anything project-scoped — agent status,
    /// container discovery. It cannot derive it: the deployment record it
    /// renders from carries `stack_id`, a UUID, and every project endpoint
    /// keys on this integer. Without the field the UI guessed from the install
    /// request and told users to deploy an agent that was already running.
    #[test]
    fn snapshot_response_serializes_project_id() {
        let resp = SnapshotResponse {
            project_id: Some(194),
            ..Default::default()
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["project_id"], 194);
    }

    /// Absent rather than null, so a client cannot mistake "unknown" for a
    /// project id of zero.
    #[test]
    fn snapshot_response_omits_absent_project_id() {
        let json = serde_json::to_value(SnapshotResponse::default()).expect("serialize");
        assert!(
            json.get("project_id").is_none(),
            "project_id must be omitted when unknown, got: {json}"
        );
    }

    fn app(code: &str) -> ProjectApp {
        ProjectApp {
            code: code.to_string(),
            ..ProjectApp::default()
        }
    }

    #[test]
    fn visible_project_apps_excludes_platform_managed_apps() {
        let apps = visible_project_apps(vec![
            app("coolify"),
            app("nginx_proxy_manager"),
            app("statuspanel"),
        ]);

        let codes = apps.iter().map(|app| app.code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes, vec!["coolify"]);
    }
}
