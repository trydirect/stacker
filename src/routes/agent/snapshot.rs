use crate::db;
use crate::helpers::{AgentPgPool, JsonResponse};
use crate::models::{Agent, Command, Deployment, ProjectApp};
use actix_web::{get, web, Responder, Result};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize, Default)]
pub struct SnapshotResponse {
    pub agent: Option<AgentSnapshot>,
    pub commands: Vec<Command>,
    pub containers: Vec<ContainerSnapshot>,
    pub apps: Vec<ProjectApp>,
}

#[derive(Debug, Serialize, Default)]
pub struct AgentSnapshot {
    pub version: Option<String>,
    pub capabilities: Option<serde_json::Value>,
    pub system_info: Option<serde_json::Value>,
    pub status: Option<String>,
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Default)]
pub struct ContainerSnapshot {
    pub id: Option<String>,
    pub app: Option<String>,
    pub state: Option<String>,
    pub image: Option<String>,
    pub name: Option<String>,
}

#[tracing::instrument(name = "Get deployment snapshot", skip(agent_pool))]
#[get("/deployments/{deployment_hash}")]
pub async fn snapshot_handler(
    path: web::Path<String>,
    agent_pool: web::Data<AgentPgPool>,
) -> Result<impl Responder> {
    tracing::info!("[SNAPSHOT HANDLER] Called for deployment_hash: {}", path);
    let deployment_hash = path.into_inner();

    // Fetch agent
    let agent = db::agent::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
        .await
        .ok()
        .flatten();

    tracing::debug!("[SNAPSHOT HANDLER] Agent : {:?}", agent);
    // Fetch commands
    let commands = db::command::fetch_by_deployment(agent_pool.get_ref(), &deployment_hash)
        .await
        .unwrap_or_default();

    tracing::debug!("[SNAPSHOT HANDLER] Commands : {:?}", commands);
    // Fetch deployment to get project_id
    let deployment = db::deployment::fetch_by_deployment_hash(agent_pool.get_ref(), &deployment_hash)
        .await
        .ok()
        .flatten();

    tracing::debug!("[SNAPSHOT HANDLER] Deployment : {:?}", deployment);
    // Fetch apps for the project
    let apps = if let Some(deployment) = &deployment {
        db::project_app::fetch_by_project(agent_pool.get_ref(), deployment.project_id)
            .await
            .unwrap_or_default()
    } else {
        vec![]
    };

    tracing::debug!("[SNAPSHOT HANDLER] Apps : {:?}", apps);
    // No container model in ProjectApp; leave containers empty for now
    let containers: Vec<ContainerSnapshot> = vec![];

    let agent_snapshot = agent.map(|a| AgentSnapshot {
        version: a.version,
        capabilities: a.capabilities,
        system_info: a.system_info,
        status: Some(a.status),
        last_heartbeat: a.last_heartbeat,
    });
    tracing::debug!("[SNAPSHOT HANDLER] Agent Snapshot : {:?}", agent_snapshot);

    let resp = SnapshotResponse {
        agent: agent_snapshot,
        commands,
        containers,
        apps,
    };

    tracing::info!("[SNAPSHOT HANDLER] Snapshot response prepared: {:?}", resp);
    Ok(JsonResponse::build().set_item(resp).ok("Snapshot fetched successfully"))
}
