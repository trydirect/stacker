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

#[derive(Debug, Serialize, Default, Clone)]
pub struct ContainerSnapshot {
    pub id: Option<String>,
    pub app: Option<String>,
    pub state: Option<String>,
    pub image: Option<String>,
    pub name: Option<String>,
    /// `project` or `platform` — who owns this container.
    ///
    /// The dashboard splits its Containers and System Containers sections on
    /// this instead of matching names, which it did with three different and
    /// disagreeing pattern lists.
    pub scope: String,
    /// Where this row came from: `registry` for one seeded from `project_app`
    /// and not yet reported on, `observed` for one the deployment has been seen
    /// running, `health` or `list_containers` for one an agent has reported.
    /// Lets the UI show "not reported yet" apart from "stopped".
    pub source: String,
    /// When an aggregate report last mentioned this container.
    ///
    /// `None` for a row that only exists because the deployment is configured
    /// to run it. Lets the dashboard say how old the news is instead of
    /// implying that silence means stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
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
/// The scope for a reported container: what the agent said, or what Stacker
/// works out from the labels it forwarded.
///
/// Agents that predate scope reporting send neither, and are classified from
/// the app code alone — which is why the label matters and why unrecognised
/// falls back to `project`.
fn reported_scope(c: &crate::forms::status_panel::HealthContainerReport) -> String {
    if let Some(scope) = c.scope.as_deref() {
        if scope == crate::helpers::stacker_labels::SCOPE_PLATFORM
            || scope == crate::helpers::stacker_labels::SCOPE_PROJECT
        {
            return scope.to_string();
        }
    }

    crate::project_app::classify_scope(
        c.labels.as_ref(),
        Some(&c.app_code),
        c.container_name.as_deref(),
        c.image.as_deref(),
    )
    .as_str()
    .to_string()
}

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
                    image: c.image.clone(),
                    name: c.container_name.clone(),
                    scope: reported_scope(c),
                    source: "health".to_string(),
                    last_seen: None,
                })
                .collect();
        }
    }

    if let Ok(single) = serde_json::from_value::<HealthCommandReport>(result.clone()) {
        // `name: None` here is why the dashboard grew a `container-3` row: it
        // invented a name for the nameless entry, so the same container
        // appeared twice under two different labels. Rows without a name are
        // merged onto the app's existing row by the caller and never stand
        // alone.
        return vec![ContainerSnapshot {
            id: None,
            app: Some(single.app_code.clone()),
            state: state_of(&single.container_state),
            image: None,
            name: None,
            scope: crate::project_app::classify_scope(None, Some(&single.app_code), None, None)
                .as_str()
                .to_string(),
            source: "health".to_string(),
            last_seen: None,
        }];
    }

    tracing::debug!(
        "health result matched neither the aggregate nor the single-app shape; ignoring"
    );
    Vec::new()
}

/// The containers an aggregate report saw, for the observed-container table.
///
/// Returns nothing for any other shape, and that restriction is the point: a
/// single-app report describes one container and says nothing about the rest,
/// so recording it as an observation of the deployment would let one per-app
/// health check rewrite what the deployment is believed to run.
pub(crate) fn observed_containers_from_report(
    result: &serde_json::Value,
) -> Vec<crate::models::ObservedContainer> {
    if result.get("type").and_then(|v| v.as_str())
        != Some(crate::forms::status_panel::ALL_HEALTH_RESULT_TYPE)
    {
        return Vec::new();
    }

    let Ok(all) = serde_json::from_value::<AllHealthCommandReport>(result.clone()) else {
        return Vec::new();
    };

    all.containers
        .iter()
        .chain(all.system_containers.iter())
        .filter_map(|c| {
            // No name, no identity: the table is keyed by container name, and a
            // row that cannot be matched again would accumulate duplicates.
            let container_name = c.container_name.clone()?;
            Some(crate::models::ObservedContainer {
                container_name,
                app_code: Some(c.app_code.clone()).filter(|code| !code.is_empty()),
                scope: reported_scope(c),
                image: c.image.clone(),
                state: serde_json::to_value(&c.container_state)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_lowercase)),
            })
        })
        .collect()
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

/// The container list a deployment shows, assembled so its *membership* is
/// stable and only the states move.
///
/// Membership has two sources, and neither is a report:
///
/// 1. `project_app` — what the deployment is *configured* to run;
/// 2. `deployment_container` — what has actually been *seen* running, which is
///    where the platform's own containers come from, since nothing configures
///    them.
///
/// Reports are laid over that and may only change state. This is the point of
/// the table: before it, membership was recomputed from the newest report on
/// every request, so a report that was missing, partial or of the wrong shape
/// changed what the user saw. Three separate causes of that were found and
/// fixed one at a time; taking membership out of the report removes the
/// category rather than the next instance.
///
/// Keyed by container name where one is known, falling back to the app code.
/// A report carrying no name at all (the single-app health shape) merges onto
/// the app's row rather than becoming a second, nameless row — which is how the
/// same agent container came to be listed twice, once as `container-3`.
///
/// **Membership comes from `latest_all_health` alone.** Only the aggregate
/// report describes the whole machine; a single-app one describes a single
/// container and says nothing about the rest. Reading whichever health command
/// finished last collapsed the list to the seeded apps whenever a per-app check
/// was newest — on dev, opening one app's health took the list from four
/// containers to two, then back, without anything on the server changing.
/// `latest_health` may therefore refresh a row that already exists, and may
/// never add or remove one.
fn assemble_containers(
    apps: &[ProjectApp],
    observed: &[crate::models::DeploymentContainer],
    latest_all_health: Option<&Command>,
    latest_health: Option<&Command>,
) -> Vec<ContainerSnapshot> {
    use std::collections::BTreeMap;

    // BTreeMap, not HashMap: a stable order is part of "the list does not jump
    // about" — HashMap iteration order varies between requests.
    let mut rows: BTreeMap<String, ContainerSnapshot> = BTreeMap::new();

    for app in apps {
        let scope = crate::project_app::classify_scope(
            app.labels.as_ref(),
            Some(&app.code),
            None,
            Some(&app.image),
        );
        rows.insert(
            app.code.clone(),
            ContainerSnapshot {
                id: None,
                app: Some(app.code.clone()),
                state: Some("unknown".to_string()),
                image: Some(app.image.clone()),
                name: None,
                scope: scope.as_str().to_string(),
                source: "registry".to_string(),
                last_seen: None,
            },
        );
    }

    // Containers seen running. Unlike a report, this does not come and go: a
    // row stays until the app is deliberately removed, so a container the
    // registry never knew about — the Status Panel, its agent, a proxy — keeps
    // its place in the list even while the deployment is silent.
    for row in observed {
        overlay(
            &mut rows,
            ContainerSnapshot {
                id: None,
                app: row.app_code.clone(),
                state: row.state.clone(),
                image: row.image.clone(),
                name: Some(row.container_name.clone()),
                scope: row.scope.clone(),
                source: "observed".to_string(),
                last_seen: Some(row.last_seen_at),
            },
            true,
        );
    }

    // The newest aggregate refreshes states. It may still add a row: a
    // container reported before the observation table existed, or written
    // moments ago by a report this request raced, should not be missing.
    if let Some(result) = latest_all_health.and_then(|cmd| cmd.result.as_ref()) {
        for reported in container_snapshots_from_health(result) {
            overlay(&mut rows, reported, true);
        }
    }

    // A newer single-app report refreshes one row. Running it when it *is* the
    // aggregate we just applied is harmless — the same values land twice.
    if let Some(result) = latest_health.and_then(|cmd| cmd.result.as_ref()) {
        for reported in container_snapshots_from_health(result) {
            overlay(&mut rows, reported, false);
        }
    }

    rows.into_values().collect()
}

/// Lay one reported container over the assembled rows.
///
/// `authoritative` separates the two callers. The aggregate report describes
/// the whole machine: it may introduce rows and it decides scope. A single-app
/// report describes one container and carries neither labels, nor an image, nor
/// a container name — it may only refresh state.
///
/// Both restrictions are load-bearing. Letting a single-app report add rows
/// makes them appear and vanish as the next aggregate arrives; letting it set
/// scope moved the Status Panel into the user's own container list, because
/// its bare app code `web` matches nothing and falls back to `project`.
fn overlay(
    rows: &mut std::collections::BTreeMap<String, ContainerSnapshot>,
    reported: ContainerSnapshot,
    authoritative: bool,
) {
    // Merge onto the seeded row when the report names an app we know,
    // otherwise the container's own name keys a new row.
    let key = match (&reported.app, &reported.name) {
        (Some(app), _) if rows.contains_key(app) => app.clone(),
        (_, Some(name)) => name.clone(),
        (Some(app), None) => app.clone(),
        (None, None) => return, // nothing to identify it by
    };

    // A row added by the aggregate is keyed by its container name, so a later
    // single-app report about the same container — which carries the app code
    // and no name — would miss it. Fall back to the app code before giving up,
    // otherwise the per-app Health button refreshes nothing.
    let key = if rows.contains_key(&key) {
        key
    } else {
        match reported
            .app
            .as_deref()
            .and_then(|code| find_by_app(rows, code))
        {
            Some(existing_key) => existing_key,
            None => key,
        }
    };

    match rows.get_mut(&key) {
        Some(existing) => {
            existing.state = reported.state.or(existing.state.take());
            existing.name = reported.name.or(existing.name.take());
            existing.image = reported.image.or(existing.image.take());
            if authoritative {
                existing.scope = reported.scope;
            }
            existing.source = reported.source;
            // A report carries no timestamp of its own, so keep the one the
            // persisted row brought rather than clearing it.
            existing.last_seen = reported.last_seen.or(existing.last_seen.take());
        }
        None if authoritative => {
            rows.insert(key, reported);
        }
        None => {}
    }
}

/// The key of the row describing this app code, if one is already present.
fn find_by_app(
    rows: &std::collections::BTreeMap<String, ContainerSnapshot>,
    app_code: &str,
) -> Option<String> {
    rows.iter()
        .find(|(_, row)| row.app.as_deref() == Some(app_code))
        .map(|(key, _)| key.clone())
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

    // Every read below propagates its failure instead of degrading into
    // "nothing found". A snapshot that cannot read the database cannot say what
    // is running, and answering 200 with an empty list told users their
    // containers were gone when the truth was that Postgres had hiccuped. An
    // error the dashboard can show beats a confident wrong answer.
    //
    // `Ok(None)` still means what it says: no agent, no deployment, no apps.
    let agent = db::agent::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
        .await
        .map_err(JsonResponse::<String>::internal_server_error)?;

    tracing::debug!("[SNAPSHOT HANDLER] Agent : {:?}", agent);
    // Fetch recent commands with optional result exclusion to reduce payload size
    let commands = db::command::fetch_recent_by_deployment(
        agent_pool.get_ref(),
        &deployment_hash,
        query.command_limit,
        !query.include_command_results,
    )
    .await
    .map_err(JsonResponse::<String>::internal_server_error)?;

    tracing::debug!("[SNAPSHOT HANDLER] Commands : {:?}", commands);
    // Fetch deployment to get project_id
    let deployment =
        db::deployment::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?;

    tracing::debug!("[SNAPSHOT HANDLER] Deployment : {:?}", deployment);
    // Fetch apps scoped to this specific deployment (falls back to project-level if no deployment-scoped apps)
    let apps = if let Some(deployment) = &deployment {
        // The seed. Losing this is what emptied the list: without the apps a
        // deployment is configured to run, membership falls back to whatever
        // the newest report happened to mention.
        db::project_app::fetch_by_deployment(
            agent_pool.get_ref(),
            deployment.project_id,
            deployment.id,
        )
        .await
        .map_err(JsonResponse::<String>::internal_server_error)?
    } else {
        vec![]
    };
    // Seeding uses every app, including platform ones: the container list shows
    // both, split by scope, while the Applications list shows only the user's.
    // Filtering here and not there is what let system containers appear among
    // the user's while being absent from their app list.
    let apps_for_seeding = apps.clone();
    let apps = visible_project_apps(apps);

    tracing::debug!("[SNAPSHOT HANDLER] Apps : {:?}", apps);

    // The newest health report, asked for directly.
    //
    // This used to scan the last 10 commands for health results, so a burst of
    // `logs` or `exec` pushed the report out of the window and the list came
    // back empty — the dashboard's containers vanished and reappeared on their
    // own. The set a user sees must not depend on what else they did recently.
    //
    // Two reports, because the two shapes answer different questions. Only the
    // aggregate says which containers exist; a single-app report says how one
    // container is doing. See [`assemble_containers`].
    let latest_all_health = db::command::fetch_latest_completed_by_type(
        agent_pool.get_ref(),
        &deployment_hash,
        "health",
        Some(crate::forms::status_panel::ALL_HEALTH_RESULT_TYPE),
    )
    .await
    .map_err(JsonResponse::<String>::internal_server_error)?;

    let latest_health = db::command::fetch_latest_completed_by_type(
        agent_pool.get_ref(),
        &deployment_hash,
        "health",
        None,
    )
    .await
    .map_err(JsonResponse::<String>::internal_server_error)?;

    // What has been seen running here. This is the deployment's membership;
    // the reports below only refresh it.
    let observed =
        db::deployment_container::fetch_by_deployment(agent_pool.get_ref(), &deployment_hash)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?;

    let containers = assemble_containers(
        &apps_for_seeding,
        &observed,
        latest_all_health.as_ref(),
        latest_health.as_ref(),
    );

    tracing::debug!(
        "[SNAPSHOT HANDLER] Containers assembled ({} rows): {:?}",
        containers.len(),
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
        .map_err(JsonResponse::<String>::internal_server_error)?;

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
            .map_err(JsonResponse::<String>::internal_server_error)?;

    let deployment =
        db::deployment::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?;

    let apps = if let Some(dep) = &deployment {
        db::project_app::fetch_by_deployment(agent_pool.get_ref(), dep.project_id, dep.id)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?
    } else {
        vec![]
    };
    let apps_for_seeding = apps.clone();
    let apps = visible_project_apps(apps);

    // The same assembly the deployment endpoint uses. This handler had kept the
    // original one — a ten-command window, keyed by app code into a HashMap,
    // with no seeding and no scope — so the dashboard silently swapped to an
    // unstable, scopeless list whenever it fell back here, which it does as soon
    // as a heartbeat looks stale. Two code paths answering the same question
    // differently is how "the containers changed by themselves" survived being
    // fixed once already.
    let latest_all_health = db::command::fetch_latest_completed_by_type(
        agent_pool.get_ref(),
        &deployment_hash,
        "health",
        Some(crate::forms::status_panel::ALL_HEALTH_RESULT_TYPE),
    )
    .await
    .map_err(JsonResponse::<String>::internal_server_error)?;

    let latest_health = db::command::fetch_latest_completed_by_type(
        agent_pool.get_ref(),
        &deployment_hash,
        "health",
        None,
    )
    .await
    .map_err(JsonResponse::<String>::internal_server_error)?;

    let observed =
        db::deployment_container::fetch_by_deployment(agent_pool.get_ref(), &deployment_hash)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?;

    let containers = assemble_containers(
        &apps_for_seeding,
        &observed,
        latest_all_health.as_ref(),
        latest_health.as_ref(),
    );

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
    use crate::models::DeploymentContainer;
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

#[cfg(test)]
mod assembly_tests {
    use super::*;
    use crate::models::DeploymentContainer;
    use serde_json::json;

    fn app(code: &str, image: &str) -> ProjectApp {
        let mut a = ProjectApp::new(1, code.to_string(), code.to_string(), image.to_string());
        a.id = 0;
        a
    }

    fn health_command(result: serde_json::Value) -> Command {
        let mut cmd = Command::default();
        cmd.r#type = "health".to_string();
        cmd.status = "completed".to_string();
        cmd.result = Some(result);
        cmd
    }

    fn all_health(containers: serde_json::Value) -> serde_json::Value {
        json!({ "type": "all_health", "deployment_hash": "d", "status": "ok",
                "containers": containers })
    }

    fn names(rows: &[ContainerSnapshot]) -> Vec<(Option<String>, String)> {
        rows.iter()
            .map(|r| (r.app.clone(), r.scope.clone()))
            .collect()
    }

    /// The requirement itself: the set of rows must not depend on whether a
    /// report happened to arrive. Only the states may differ.
    #[test]
    fn membership_is_the_same_with_and_without_a_report() {
        let apps = vec![
            app("floci", "floci/floci:latest"),
            app("floci-ui", "floci/floci-ui:latest"),
        ];

        let without = assemble_containers(&apps, &[], None, None);
        let cmd = health_command(all_health(json!([
            { "app_code": "floci", "container_name": "project-app-1",
              "container_state": "running", "status": "ok" }
        ])));
        let with = assemble_containers(&apps, &[], Some(&cmd), Some(&cmd));

        assert_eq!(names(&without), names(&with), "the row set must not move");
        assert_eq!(without.len(), 2);

        let reported = with
            .iter()
            .find(|r| r.app.as_deref() == Some("floci"))
            .unwrap();
        assert_eq!(reported.state.as_deref(), Some("running"));
        let silent = with
            .iter()
            .find(|r| r.app.as_deref() == Some("floci-ui"))
            .unwrap();
        assert_eq!(
            silent.state.as_deref(),
            Some("unknown"),
            "unreported means unknown, not gone"
        );
    }

    /// A report that mentions nothing must not empty the list.
    #[test]
    fn an_empty_report_does_not_remove_rows() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let cmd = health_command(all_health(json!([])));

        let rows = assemble_containers(&apps, &[], Some(&cmd), Some(&cmd));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state.as_deref(), Some("unknown"));
    }

    /// The single-app health shape carries no container name. It used to become
    /// its own nameless row, which the dashboard rendered as `container-3` —
    /// the same agent listed twice.
    #[test]
    fn a_nameless_report_merges_instead_of_adding_a_row() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let cmd = health_command(json!({
            "type": "health", "app_code": "floci", "deployment_hash": "d",
            "container_state": "running", "status": "ok"
        }));

        let rows = assemble_containers(&apps, &[], None, Some(&cmd));

        assert_eq!(rows.len(), 1, "no second, nameless row");
        assert_eq!(rows[0].state.as_deref(), Some("running"));
    }

    /// A container the agent reports that is not in the registry still shows —
    /// it is really running, and hiding it would be its own kind of surprise.
    #[test]
    fn unregistered_containers_are_added_not_dropped() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let cmd = health_command(all_health(json!([
            { "app_code": "floci", "container_name": "project-app-1",
              "container_state": "running", "status": "ok" },
            { "app_code": "statuspanel", "container_name": "statuspanel",
              "container_state": "running", "status": "ok" }
        ])));

        let rows = assemble_containers(&apps, &[], Some(&cmd), Some(&cmd));

        assert_eq!(rows.len(), 2);
        let panel = rows
            .iter()
            .find(|r| r.app.as_deref() == Some("statuspanel"))
            .unwrap();
        assert_eq!(
            panel.scope, "platform",
            "the panel is the platform's, not the user's"
        );
    }

    /// Only the aggregate speaks for the deployment. A per-app report describes
    /// one container, so recording it as an observation would let a single
    /// Health click rewrite what the deployment is believed to run.
    #[test]
    fn only_an_aggregate_report_counts_as_an_observation() {
        let single = json!({
            "type": "health", "app_code": "web", "deployment_hash": "d",
            "container_state": "running", "status": "ok"
        });

        assert!(observed_containers_from_report(&single).is_empty());
        assert!(observed_containers_from_report(&json!({"nonsense": true})).is_empty());
    }

    #[test]
    fn an_aggregate_report_yields_both_lists_with_their_scope() {
        let result = json!({
            "type": "all_health", "deployment_hash": "d", "status": "ok",
            "containers": [
                { "app_code": "floci", "container_name": "project-app-1",
                  "container_state": "running", "status": "ok",
                  "image": "floci/floci:latest" }
            ],
            "system_containers": [
                { "app_code": "web", "container_name": "statuspanel",
                  "container_state": "running", "status": "ok",
                  "scope": "platform" }
            ]
        });

        let observed = observed_containers_from_report(&result);

        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].container_name, "project-app-1");
        assert_eq!(observed[0].scope, "project");
        assert_eq!(observed[0].image.as_deref(), Some("floci/floci:latest"));
        assert_eq!(observed[0].state.as_deref(), Some("running"));
        assert_eq!(observed[1].container_name, "statuspanel");
        assert_eq!(observed[1].scope, "platform");
    }

    /// The table is keyed by container name, so a nameless entry cannot be
    /// matched again and would accumulate a fresh row on every report.
    #[test]
    fn a_container_without_a_name_is_not_recorded() {
        let result = json!({
            "type": "all_health", "deployment_hash": "d", "status": "ok",
            "containers": [
                { "app_code": "floci", "container_state": "running", "status": "ok" },
                { "app_code": "floci-ui", "container_name": "project-floci-ui-1",
                  "container_state": "running", "status": "ok" }
            ]
        });

        let observed = observed_containers_from_report(&result);

        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].container_name, "project-floci-ui-1");
    }

    /// An unlabelled platform container is still the platform's: the classifier
    /// recognises it by name, which is what keeps old agents working.
    #[test]
    fn an_unlabelled_platform_container_is_still_classified() {
        let result = json!({
            "type": "all_health", "deployment_hash": "d", "status": "ok",
            "containers": [
                { "app_code": "agent", "container_name": "statuspanel_agent",
                  "container_state": "running", "status": "ok" }
            ]
        });

        let observed = observed_containers_from_report(&result);

        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].scope, "platform");
        assert_eq!(observed[0].app_code.as_deref(), Some("agent"));
    }

    /// Caught on dev: opening one app's health took the container list from
    /// four rows to two and back. A single-app report describes one container
    /// and knows nothing about the rest, so it must never define the set.
    #[test]
    fn a_single_app_report_does_not_shrink_the_list() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let aggregate = health_command(all_health(json!([
            { "app_code": "floci", "container_name": "project-app-1",
              "container_state": "running", "status": "ok" },
            { "app_code": "agent", "container_name": "statuspanel_agent",
              "container_state": "running", "status": "ok" }
        ])));
        let single = health_command(json!({
            "type": "health", "app_code": "floci", "deployment_hash": "d",
            "container_state": "exited", "status": "ok"
        }));

        let rows = assemble_containers(&apps, &[], Some(&aggregate), Some(&single));

        assert_eq!(rows.len(), 2, "the platform row must survive");
        let panel = rows
            .iter()
            .find(|r| r.name.as_deref() == Some("statuspanel_agent"))
            .expect("the container the single-app report says nothing about");
        assert_eq!(panel.scope, "platform");
        assert_eq!(panel.state.as_deref(), Some("running"));

        let floci = rows
            .iter()
            .find(|r| r.app.as_deref() == Some("floci"))
            .unwrap();
        assert_eq!(
            floci.state.as_deref(),
            Some("exited"),
            "the newer report still refreshes the row it does describe"
        );
    }

    /// The Health button on a system container asks about one app. Its row was
    /// keyed by container name when the aggregate added it, so matching by key
    /// alone missed it and the click appeared to do nothing.
    #[test]
    fn a_single_app_report_refreshes_a_row_keyed_by_container_name() {
        let aggregate = health_command(all_health(json!([
            { "app_code": "web", "container_name": "statuspanel",
              "container_state": "running", "status": "ok" }
        ])));
        let single = health_command(json!({
            "type": "health", "app_code": "web", "deployment_hash": "d",
            "container_state": "exited", "status": "unhealthy"
        }));

        let rows = assemble_containers(&[], &[], Some(&aggregate), Some(&single));

        assert_eq!(rows.len(), 1, "still one container, not two");
        assert_eq!(rows[0].name.as_deref(), Some("statuspanel"));
        assert_eq!(rows[0].state.as_deref(), Some("exited"));
        assert_eq!(rows[0].scope, "platform", "and still the platform's");
    }

    /// A per-app report about something no aggregate has mentioned must not
    /// conjure a row that the next aggregate takes away again.
    #[test]
    fn a_single_app_report_does_not_add_a_row() {
        let cmd = health_command(json!({
            "type": "health", "app_code": "stranger", "deployment_hash": "d",
            "container_state": "running", "status": "ok"
        }));

        let rows = assemble_containers(&[], &[], None, Some(&cmd));

        assert!(rows.is_empty(), "membership comes from the aggregate alone");
    }

    /// The agent's own classification wins when it sends one.
    #[test]
    fn the_reported_scope_is_respected() {
        let cmd = health_command(all_health(json!([
            { "app_code": "anything", "container_name": "anything",
              "container_state": "running", "status": "ok", "scope": "platform" }
        ])));

        let rows = assemble_containers(&[], &[], Some(&cmd), Some(&cmd));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].scope, "platform");
    }

    fn seen(name: &str, app_code: &str, scope: &str, state: &str) -> DeploymentContainer {
        let now = chrono::Utc::now();
        DeploymentContainer {
            id: 0,
            deployment_hash: "d".to_string(),
            container_name: name.to_string(),
            app_code: Some(app_code.to_string()),
            scope: scope.to_string(),
            image: None,
            state: Some(state.to_string()),
            first_seen_at: now,
            last_seen_at: now,
            removed_at: None,
        }
    }

    /// The requirement, now met without any report at all. A container the
    /// registry knows nothing about — the platform's own — used to exist only
    /// for as long as a report mentioned it.
    #[test]
    fn a_seen_container_is_listed_with_no_report_at_all() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let observed = vec![
            seen("project-app-1", "floci", "project", "running"),
            seen("statuspanel", "web", "platform", "running"),
        ];

        let rows = assemble_containers(&apps, &observed, None, None);

        assert_eq!(rows.len(), 2, "one app, one platform container");
        let panel = rows
            .iter()
            .find(|r| r.name.as_deref() == Some("statuspanel"))
            .expect("the platform container nothing configures");
        assert_eq!(panel.scope, "platform");
        assert_eq!(panel.source, "observed");
        assert!(panel.last_seen.is_some(), "the row dates itself");
    }

    /// A container that was seen and is not in the newest report keeps its
    /// place. This is the failure the table exists to end: the agent goes
    /// quiet, or lists incompletely, and the dashboard used to lose rows.
    #[test]
    fn a_seen_container_survives_a_report_that_omits_it() {
        let observed = vec![
            seen("project-app-1", "floci", "project", "running"),
            seen("statuspanel", "web", "platform", "running"),
        ];
        let cmd = health_command(all_health(json!([
            { "app_code": "floci", "container_name": "project-app-1",
              "container_state": "running", "status": "ok" }
        ])));

        let rows = assemble_containers(&[], &observed, Some(&cmd), Some(&cmd));

        assert_eq!(rows.len(), 2, "the omitted container is still listed");
        let panel = rows
            .iter()
            .find(|r| r.name.as_deref() == Some("statuspanel"))
            .expect("the container the report said nothing about");
        assert_eq!(
            panel.state.as_deref(),
            Some("running"),
            "its last known state, not a guess"
        );
    }

    /// A row seeded from the registry and one seen running are the same
    /// container, not two.
    #[test]
    fn a_configured_app_and_its_container_are_one_row() {
        let apps = vec![app("floci", "floci/floci:latest")];
        let observed = vec![seen("project-app-1", "floci", "project", "running")];

        let rows = assemble_containers(&apps, &observed, None, None);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name.as_deref(), Some("project-app-1"));
        assert_eq!(rows[0].app.as_deref(), Some("floci"));
        assert_eq!(rows[0].state.as_deref(), Some("running"));
    }

    /// The freshest report still wins on state — the table says what exists,
    /// not how it is doing right now.
    #[test]
    fn a_report_refreshes_the_state_of_a_seen_container() {
        let observed = vec![seen("project-app-1", "floci", "project", "running")];
        let cmd = health_command(all_health(json!([
            { "app_code": "floci", "container_name": "project-app-1",
              "container_state": "exited", "status": "unhealthy" }
        ])));

        let rows = assemble_containers(&[], &observed, Some(&cmd), Some(&cmd));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state.as_deref(), Some("exited"));
        assert!(
            rows[0].last_seen.is_some(),
            "a report must not erase when the container was last seen"
        );
    }

    /// Order is part of stability: rows must not shuffle between requests.
    #[test]
    fn rows_come_back_in_a_stable_order() {
        let apps = vec![app("zeta", "z:1"), app("alpha", "a:1"), app("mu", "m:1")];

        let first = assemble_containers(&apps, &[], None, None);
        let second = assemble_containers(&apps, &[], None, None);

        assert_eq!(names(&first), names(&second));
        assert_eq!(
            first
                .iter()
                .filter_map(|r| r.app.clone())
                .collect::<Vec<_>>(),
            vec!["alpha", "mu", "zeta"]
        );
    }
}
