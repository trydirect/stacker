//! Container Discovery & Import API
//!
//! Endpoints for discovering running containers and importing them into project_app table.
//! This allows users to register containers that are running but not tracked in the database.

use crate::db;
use crate::helpers::JsonResponse;
use crate::models::{self, ProjectApp};
use actix_web::{get, post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

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
#[derive(Debug, Serialize)]
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
#[tracing::instrument(name = "Discover containers", skip(pg_pool))]
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
    
    // Get deployment_hash from query or find it from project
    let deployment_hash = match &query.deployment_hash {
        Some(hash) => hash.clone(),
        None => {
            // Try to find a deployment for this project
            let deployments = db::deployment::fetch_by_project_id(pg_pool.get_ref(), project_id)
                .await
                .map_err(|e| JsonResponse::internal_server_error(e))?;
            
            deployments.first()
                .map(|d| d.deployment_hash.clone())
                .ok_or_else(|| JsonResponse::not_found("No deployment found for project. Please provide deployment_hash"))?
        }
    };
    
    // Fetch all apps registered in this project
    let registered_apps = db::project_app::fetch_by_project(pg_pool.get_ref(), project_id)
        .await
        .map_err(|e| JsonResponse::internal_server_error(e))?;
    
    // Fetch recent health check commands to get actual container states
    let health_commands = db::command::fetch_recent_by_deployment(
        pg_pool.get_ref(),
        &deployment_hash,
        20, // Last 20 commands to ensure we get recent health checks for all containers
        false, // Include results
    )
    .await
    .unwrap_or_default();
    
    // Extract running containers from health checks
    let mut running_containers: Vec<ContainerInfo> = Vec::new();
    
    for cmd in health_commands.iter() {
        if cmd.r#type == "health" && cmd.status == "completed" {
            if let Some(result) = &cmd.result {
                // Parse the health check result
                if let Ok(health) = serde_json::from_value::<crate::forms::status_panel::HealthCommandReport>(result.clone()) {
                    // Extract container info
                    let container_name = health.container_name.unwrap_or_else(|| health.app_code.clone());
                    let image = health.image.unwrap_or_default();
                    let status = format!("{:?}", health.container_state).to_lowercase();
                    
                    // Check if we already have this container
                    if !running_containers.iter().any(|c| c.name == container_name) {
                        running_containers.push(ContainerInfo {
                            name: container_name,
                            image,
                            status,
                            app_code: Some(health.app_code.clone()),
                        });
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
    
    // Classify containers
    let mut registered = Vec::new();
    let mut unregistered = Vec::new();
    let mut missing_containers = Vec::new();
    
    // Find registered apps with running containers
    for app in &registered_apps {
        let matching_container = running_containers.iter()
            .find(|c| {
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
        let is_registered = registered_apps.iter()
            .any(|app| {
                app.code == container.app_code.clone().unwrap_or_default() ||
                container_matches_app(&container.name, &app.code)
            });
        
        if !is_registered {
            let (suggested_code, suggested_name) = suggest_app_info(&container.name, &container.image);
            
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
#[tracing::instrument(name = "Import containers", skip(pg_pool, body))]
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
        // Check if app_code already exists
        let existing = db::project_app::fetch_by_project_and_code(
            pg_pool.get_ref(),
            project_id,
            &container.app_code
        ).await.ok().flatten();
        
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
