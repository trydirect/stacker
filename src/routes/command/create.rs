use crate::configuration::Settings;
use crate::db;
use crate::forms::status_panel;
use crate::helpers::project::builder::parse_compose_services;
use crate::helpers::JsonResponse;
use crate::models::{Command, CommandPriority, User};
use crate::project_app::{store_configs_to_vault_from_params, upsert_app_config_for_deploy};
use crate::services::VaultService;
use actix_web::{post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct CreateCommandRequest {
    pub deployment_hash: String,
    pub command_type: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub timeout_seconds: Option<i32>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Default)]
pub struct CreateCommandResponse {
    pub command_id: String,
    pub deployment_hash: String,
    pub status: String,
}

#[tracing::instrument(name = "Create command", skip(pg_pool, user, settings))]
#[post("")]
pub async fn create_handler(
    user: web::ReqData<Arc<User>>,
    req: web::Json<CreateCommandRequest>,
    pg_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
) -> Result<impl Responder> {
    tracing::info!(
        "[CREATE COMMAND HANDLER] User: {}, Deployment: {}, Command Type: {}",
        user.id,
        req.deployment_hash,
        req.command_type
    );
    if req.deployment_hash.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("deployment_hash is required"));
    }

    if req.command_type.trim().is_empty() {
        return Err(JsonResponse::<()>::build().bad_request("command_type is required"));
    }

    let validated_parameters =
        status_panel::validate_command_parameters(&req.command_type, &req.parameters).map_err(
            |err| {
                tracing::warn!("Invalid command payload: {}", err);
                JsonResponse::<()>::build().bad_request(err)
            },
        )?;

    // For deploy_app commands, upsert app config and sync to Vault before enriching parameters
    let final_parameters = if req.command_type == "deploy_app" {
        // Try to get deployment_id from parameters, or look it up by deployment_hash
        // If no deployment exists, auto-create project and deployment records
        let deployment_id = match req
            .parameters
            .as_ref()
            .and_then(|p| p.get("deployment_id"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
        {
            Some(id) => Some(id),
            None => {
                // Auto-lookup project_id from deployment_hash
                match crate::db::deployment::fetch_by_deployment_hash(
                    pg_pool.get_ref(),
                    &req.deployment_hash,
                )
                .await
                {
                    Ok(Some(deployment)) => {
                        tracing::debug!(
                            "Auto-resolved project_id {} from deployment_hash {}",
                            deployment.project_id,
                            &req.deployment_hash
                        );
                        Some(deployment.project_id)
                    }
                    Ok(None) => {
                        // No deployment found - auto-create project and deployment
                        tracing::info!(
                            "No deployment found for hash {}, auto-creating project and deployment",
                            &req.deployment_hash
                        );

                        // Get app_code to use as project name
                        let app_code_for_name = req
                            .parameters
                            .as_ref()
                            .and_then(|p| p.get("app_code"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("project");

                        // Create project
                        let project = crate::models::Project::new(
                            user.id.clone(),
                            app_code_for_name.to_string(),
                            serde_json::json!({"auto_created": true, "deployment_hash": &req.deployment_hash}),
                            req.parameters.clone().unwrap_or(serde_json::json!({})),
                        );

                        match crate::db::project::insert(pg_pool.get_ref(), project).await {
                            Ok(created_project) => {
                                tracing::info!(
                                    "Auto-created project {} (id={}) for deployment_hash {}",
                                    created_project.name,
                                    created_project.id,
                                    &req.deployment_hash
                                );

                                // Create deployment linked to this project
                                let deployment = crate::models::Deployment::new(
                                    created_project.id,
                                    Some(user.id.clone()),
                                    req.deployment_hash.clone(),
                                    "pending".to_string(),
                                    serde_json::json!({"auto_created": true}),
                                );

                                match crate::db::deployment::insert(pg_pool.get_ref(), deployment)
                                    .await
                                {
                                    Ok(created_deployment) => {
                                        tracing::info!(
                                            "Auto-created deployment (id={}) linked to project {}",
                                            created_deployment.id,
                                            created_project.id
                                        );
                                        Some(created_project.id)
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to auto-create deployment: {}", e);
                                        // Project was created, return its ID anyway
                                        Some(created_project.id)
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to auto-create project: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to lookup deployment by hash: {}", e);
                        None
                    }
                }
            }
        };

        let app_code = req
            .parameters
            .as_ref()
            .and_then(|p| p.get("app_code"))
            .and_then(|v| v.as_str());
        let app_params = req.parameters.as_ref().and_then(|p| p.get("parameters"));

        // CRITICAL: Log incoming parameters for debugging env/config save issues
        tracing::info!(
            "[DEPLOY_APP] deployment_id: {:?}, app_code: {:?}, has_app_params: {}, raw_params: {}",
            deployment_id,
            app_code,
            app_params.is_some(),
            req.parameters
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_else(|| "None".to_string())
        );

        if let Some(params) = app_params.or(req.parameters.as_ref()) {
            tracing::info!(
                "[DEPLOY_APP] Parameters contain - env: {}, config_files: {}, image: {}",
                params
                    .get("env")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "None".to_string()),
                params
                    .get("config_files")
                    .map(|v| format!("{} files", v.as_array().map(|a| a.len()).unwrap_or(0)))
                    .unwrap_or_else(|| "None".to_string()),
                params
                    .get("image")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "None".to_string())
            );
        }

        tracing::debug!(
            "deploy_app command detected, upserting app config for deployment_id: {:?}, app_code: {:?}",
            deployment_id,
            app_code
        );
        if let (Some(deployment_id), Some(app_code), Some(app_params)) =
            (deployment_id, app_code, app_params)
        {
            upsert_app_config_for_deploy(
                pg_pool.get_ref(),
                deployment_id,
                app_code,
                app_params,
                &req.deployment_hash,
            )
            .await;
        } else if let (Some(deployment_id), Some(app_code)) = (deployment_id, app_code) {
            // Have deployment_id and app_code but no nested parameters - use top-level parameters
            if let Some(params) = req.parameters.as_ref() {
                upsert_app_config_for_deploy(
                    pg_pool.get_ref(),
                    deployment_id,
                    app_code,
                    params,
                    &req.deployment_hash,
                )
                .await;
            }
        } else if let Some(app_code) = app_code {
            // No deployment_id available (auto-create failed), just store to Vault
            if let Some(params) = req.parameters.as_ref() {
                store_configs_to_vault_from_params(
                    params,
                    &req.deployment_hash,
                    app_code,
                    &settings.vault,
                    &settings.deployment,
                )
                .await;
            }
        } else {
            tracing::warn!("Missing app_code in deploy_app arguments");
        }

        let enriched_params = enrich_deploy_app_with_compose(
            &req.deployment_hash,
            validated_parameters,
            &settings.vault,
        )
        .await;

        // Auto-discover child services from multi-service compose files
        if let (Some(project_id), Some(app_code)) = (deployment_id, app_code) {
            if let Some(compose_content) = enriched_params
                .as_ref()
                .and_then(|p| p.get("compose_content"))
                .and_then(|c| c.as_str())
            {
                discover_and_register_child_services(
                    pg_pool.get_ref(),
                    project_id,
                    app_code,
                    compose_content,
                )
                .await;
            }
        }

        enriched_params
    } else {
        validated_parameters
    };

    // Generate unique command ID
    let command_id = format!("cmd_{}", uuid::Uuid::new_v4());

    // Parse priority or default to Normal
    let priority = req
        .priority
        .as_ref()
        .and_then(|p| match p.to_lowercase().as_str() {
            "low" => Some(CommandPriority::Low),
            "normal" => Some(CommandPriority::Normal),
            "high" => Some(CommandPriority::High),
            "critical" => Some(CommandPriority::Critical),
            _ => None,
        })
        .unwrap_or(CommandPriority::Normal);

    // Build command
    let mut command = Command::new(
        command_id.clone(),
        req.deployment_hash.clone(),
        req.command_type.clone(),
        user.id.clone(),
    )
    .with_priority(priority.clone());

    if let Some(params) = &final_parameters {
        command = command.with_parameters(params.clone());
    }

    if let Some(timeout) = req.timeout_seconds {
        command = command.with_timeout(timeout);
    }

    if let Some(metadata) = &req.metadata {
        command = command.with_metadata(metadata.clone());
    }

    // Insert command into database
    let saved_command = db::command::insert(pg_pool.get_ref(), &command)
        .await
        .map_err(|err| {
            tracing::error!("Failed to create command: {}", err);
            JsonResponse::<()>::build().internal_server_error(err)
        })?;

    // Add to queue - agent will poll and pick it up
    db::command::add_to_queue(
        pg_pool.get_ref(),
        &saved_command.command_id,
        &saved_command.deployment_hash,
        &priority,
    )
    .await
    .map_err(|err| {
        tracing::error!("Failed to add command to queue: {}", err);
        JsonResponse::<()>::build().internal_server_error(err)
    })?;

    tracing::info!(
        command_id = %saved_command.command_id,
        deployment_hash = %saved_command.deployment_hash,
        "Command created and queued, agent will poll"
    );

    let response = CreateCommandResponse {
        command_id: saved_command.command_id,
        deployment_hash: saved_command.deployment_hash,
        status: saved_command.status,
    };

    Ok(JsonResponse::build()
        .set_item(Some(response))
        .created("Command created successfully"))
}

/// Enrich deploy_app command parameters with compose_content and config_files from Vault
/// Falls back to fetching templates from Install Service if not in Vault
/// If compose_content is already provided in the request, keep it as-is
async fn enrich_deploy_app_with_compose(
    deployment_hash: &str,
    params: Option<serde_json::Value>,
    vault_settings: &crate::configuration::VaultSettings,
) -> Option<serde_json::Value> {
    let mut params = params.unwrap_or_else(|| json!({}));

    // Get app_code from parameters - compose is stored under app_code key in Vault
    // Clone to avoid borrowing params while we need to mutate it later
    let app_code = params
        .get("app_code")
        .and_then(|v| v.as_str())
        .unwrap_or("_compose")
        .to_string();

    // Initialize Vault client
    let vault = match VaultService::from_settings(vault_settings) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "Failed to initialize Vault: {}, cannot enrich deploy_app",
                e
            );
            return Some(params);
        }
    };

    // If compose_content is not already provided, fetch from Vault
    if params
        .get("compose_content")
        .and_then(|v| v.as_str())
        .is_none()
    {
        tracing::debug!(
            deployment_hash = %deployment_hash,
            app_code = %app_code,
            "Looking up compose content in Vault"
        );

        // Fetch compose config - stored under app_code key (e.g., "telegraf")
        match vault.fetch_app_config(deployment_hash, &app_code).await {
            Ok(compose_config) => {
                tracing::info!(
                    deployment_hash = %deployment_hash,
                    app_code = %app_code,
                    "Enriched deploy_app command with compose_content from Vault"
                );
                if let Some(obj) = params.as_object_mut() {
                    obj.insert("compose_content".to_string(), json!(compose_config.content));
                }
            }
            Err(e) => {
                tracing::warn!(
                    deployment_hash = %deployment_hash,
                    app_code = %app_code,
                    error = %e,
                    "Failed to fetch compose from Vault, deploy_app may fail if compose not on disk"
                );
            }
        }
    } else {
        tracing::debug!("deploy_app already has compose_content, skipping Vault fetch");
    }

    // Collect config files from Vault (bundled configs, legacy single config, and .env files)
    let mut config_files: Vec<serde_json::Value> = Vec::new();

    // If config_files already provided, use them
    if let Some(existing_configs) = params.get("config_files").and_then(|v| v.as_array()) {
        config_files.extend(existing_configs.iter().cloned());
    }

    // Try to fetch bundled config files from Vault (new format: "{app_code}_configs")
    let configs_key = format!("{}_configs", app_code);
    tracing::debug!(
        deployment_hash = %deployment_hash,
        configs_key = %configs_key,
        "Looking up bundled config files in Vault"
    );

    match vault.fetch_app_config(deployment_hash, &configs_key).await {
        Ok(bundle_config) => {
            // Parse the JSON array of configs
            if let Ok(configs_array) =
                serde_json::from_str::<Vec<serde_json::Value>>(&bundle_config.content)
            {
                tracing::info!(
                    deployment_hash = %deployment_hash,
                    app_code = %app_code,
                    config_count = configs_array.len(),
                    "Found bundled config files in Vault"
                );
                config_files.extend(configs_array);
            } else {
                tracing::warn!(
                    deployment_hash = %deployment_hash,
                    app_code = %app_code,
                    "Failed to parse bundled config files from Vault"
                );
            }
        }
        Err(_) => {
            // Fall back to legacy single config format ("{app_code}_config")
            let config_key = format!("{}_config", app_code);
            tracing::debug!(
                deployment_hash = %deployment_hash,
                config_key = %config_key,
                "Looking up legacy single config file in Vault"
            );

            match vault.fetch_app_config(deployment_hash, &config_key).await {
                Ok(app_config) => {
                    tracing::info!(
                        deployment_hash = %deployment_hash,
                        app_code = %app_code,
                        destination = %app_config.destination_path,
                        "Found app config file in Vault"
                    );
                    // Convert AppConfig to the format expected by status panel
                    let config_file = json!({
                        "content": app_config.content,
                        "content_type": app_config.content_type,
                        "destination_path": app_config.destination_path,
                        "file_mode": app_config.file_mode,
                        "owner": app_config.owner,
                        "group": app_config.group,
                    });
                    config_files.push(config_file);
                }
                Err(e) => {
                    tracing::debug!(
                        deployment_hash = %deployment_hash,
                        config_key = %config_key,
                        error = %e,
                        "No app config found in Vault (this is normal for apps without config files)"
                    );
                }
            }
        }
    }

    // Also fetch .env file from Vault (stored under "{app_code}_env" key)
    let env_key = format!("{}_env", app_code);
    tracing::debug!(
        deployment_hash = %deployment_hash,
        env_key = %env_key,
        "Looking up .env file in Vault"
    );

    match vault.fetch_app_config(deployment_hash, &env_key).await {
        Ok(env_config) => {
            tracing::info!(
                deployment_hash = %deployment_hash,
                app_code = %app_code,
                destination = %env_config.destination_path,
                "Found .env file in Vault"
            );
            // Convert AppConfig to the format expected by status panel
            let env_file = json!({
                "content": env_config.content,
                "content_type": env_config.content_type,
                "destination_path": env_config.destination_path,
                "file_mode": env_config.file_mode,
                "owner": env_config.owner,
                "group": env_config.group,
            });
            config_files.push(env_file);
        }
        Err(e) => {
            tracing::debug!(
                deployment_hash = %deployment_hash,
                env_key = %env_key,
                error = %e,
                "No .env file found in Vault (this is normal for apps without environment config)"
            );
        }
    }

    // Insert config_files into params if we found any
    if !config_files.is_empty() {
        tracing::info!(
            deployment_hash = %deployment_hash,
            app_code = %app_code,
            config_count = config_files.len(),
            "Enriched deploy_app command with config_files from Vault"
        );
        if let Some(obj) = params.as_object_mut() {
            obj.insert("config_files".to_string(), json!(config_files));
        }
    }

    Some(params)
}

/// Discover child services from a multi-service compose file and register them as project_apps.
/// This is called after deploy_app enrichment to auto-create entries for stacks like Komodo
/// that have multiple services (core, ferretdb, periphery).
///
/// Returns the number of child services discovered and registered.
pub async fn discover_and_register_child_services(
    pg_pool: &PgPool,
    project_id: i32,
    parent_app_code: &str,
    compose_content: &str,
) -> usize {
    // Parse the compose file to extract services
    let services = match parse_compose_services(compose_content) {
        Ok(svcs) => svcs,
        Err(e) => {
            tracing::debug!(
                parent_app = %parent_app_code,
                error = %e,
                "Failed to parse compose for service discovery (may be single-service)"
            );
            return 0;
        }
    };

    // If only 1 service, no child discovery needed
    if services.len() <= 1 {
        tracing::debug!(
            parent_app = %parent_app_code,
            services_count = services.len(),
            "Single service compose, no child discovery needed"
        );
        return 0;
    }

    tracing::info!(
        parent_app = %parent_app_code,
        services_count = services.len(),
        services = ?services.iter().map(|s| &s.name).collect::<Vec<_>>(),
        "Multi-service compose detected, auto-discovering child services"
    );

    let mut registered_count = 0;

    for svc in &services {
        // Generate unique code: parent_code-service_name
        let app_code = format!("{}-{}", parent_app_code, svc.name);

        // Check if already exists
        match db::project_app::fetch_by_project_and_code(pg_pool, project_id, &app_code).await {
            Ok(Some(_)) => {
                tracing::debug!(
                    app_code = %app_code,
                    "Child service already registered, skipping"
                );
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    app_code = %app_code,
                    error = %e,
                    "Failed to check if child service exists"
                );
                continue;
            }
        }

        tracing::debug!(
            app_code = %app_code,
            service = %svc.name,
            project_id = %project_id,
            "Processing child service for registration"
        );
        // Create new project_app for this service
        let mut new_app = crate::models::ProjectApp::new(
            project_id,
            app_code.clone(),
            svc.name.clone(),
            svc.image.clone().unwrap_or_else(|| "unknown".to_string()),
        );

        // Set parent reference
        new_app.parent_app_code = Some(parent_app_code.to_string());

        // Convert environment to JSON object
        if !svc.environment.is_empty() {
            let mut env_map = serde_json::Map::new();
            for env_str in &svc.environment {
                if let Some((k, v)) = env_str.split_once('=') {
                    env_map.insert(k.to_string(), json!(v));
                }
            }
            new_app.environment = Some(json!(env_map));
        }

        // Convert ports to JSON array
        if !svc.ports.is_empty() {
            new_app.ports = Some(json!(svc.ports));
        }

        // Convert volumes to JSON array
        if !svc.volumes.is_empty() {
            new_app.volumes = Some(json!(svc.volumes));
        }

        // Set networks
        if !svc.networks.is_empty() {
            new_app.networks = Some(json!(svc.networks));
        }

        // Set depends_on
        if !svc.depends_on.is_empty() {
            new_app.depends_on = Some(json!(svc.depends_on));
        }

        // Set command and entrypoint
        new_app.command = svc.command.clone();
        new_app.entrypoint = svc.entrypoint.clone();
        new_app.restart_policy = svc.restart.clone();

        // Convert labels to JSON
        if !svc.labels.is_empty() {
            let labels_map: serde_json::Map<String, serde_json::Value> = svc
                .labels
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            new_app.labels = Some(json!(labels_map));
        }

        // Insert into database
        match db::project_app::insert(pg_pool, &new_app).await {
            Ok(created) => {
                tracing::info!(
                    app_code = %app_code,
                    id = created.id,
                    service = %svc.name,
                    image = ?svc.image,
                    "Auto-registered child service from compose"
                );
                registered_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    app_code = %app_code,
                    service = %svc.name,
                    error = %e,
                    "Failed to register child service"
                );
            }
        }
    }

    if registered_count > 0 {
        tracing::info!(
            parent_app = %parent_app_code,
            registered_count = registered_count,
            "Successfully auto-registered child services"
        );
    }

    registered_count
}
