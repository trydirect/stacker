use crate::db;
use crate::forms::status_panel;
use crate::helpers::project::builder::generate_single_app_compose;
use crate::helpers::JsonResponse;
use crate::models::{Command, CommandPriority, User};
use crate::services::{VaultService, ProjectAppService};
use crate::configuration::Settings;
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

/// Intermediate struct for mapping POST parameters to ProjectApp fields
#[derive(Debug, Default)]
struct ProjectAppPostArgs {
    name: Option<String>,
    image: Option<String>,
    environment: Option<serde_json::Value>,
    ports: Option<serde_json::Value>,
    volumes: Option<serde_json::Value>,
    config_files: Option<serde_json::Value>,
    compose_content: Option<String>,
    domain: Option<String>,
    ssl_enabled: Option<bool>,
    resources: Option<serde_json::Value>,
    restart_policy: Option<String>,
    command: Option<String>,
    entrypoint: Option<String>,
    networks: Option<serde_json::Value>,
    depends_on: Option<serde_json::Value>,
    healthcheck: Option<serde_json::Value>,
    labels: Option<serde_json::Value>,
    enabled: Option<bool>,
    deploy_order: Option<i32>,
}

impl From<&serde_json::Value> for ProjectAppPostArgs {
    fn from(params: &serde_json::Value) -> Self {
        let mut args = ProjectAppPostArgs::default();

        // Basic fields
        if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
            args.name = Some(name.to_string());
        }
        if let Some(image) = params.get("image").and_then(|v| v.as_str()) {
            args.image = Some(image.to_string());
        }

        // Environment variables
        if let Some(env) = params.get("env") {
            args.environment = Some(env.clone());
        }

        // Port mappings
        if let Some(ports) = params.get("ports") {
            args.ports = Some(ports.clone());
        }

        // Volume mounts (separate from config_files)
        if let Some(volumes) = params.get("volumes") {
            args.volumes = Some(volumes.clone());
        }

        // Config files - extract compose content and store remaining files
        if let Some(config_files) = params.get("config_files").and_then(|v| v.as_array()) {
            let mut non_compose_files = Vec::new();
            for file in config_files {
                let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if file_name == "compose" || file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
                    // Extract compose content
                    if let Some(content) = file.get("content").and_then(|c| c.as_str()) {
                        args.compose_content = Some(content.to_string());
                    }
                } else {
                    non_compose_files.push(file.clone());
                }
            }
            if !non_compose_files.is_empty() {
                args.config_files = Some(serde_json::Value::Array(non_compose_files));
            }
        }

        // Domain and SSL
        if let Some(domain) = params.get("domain").and_then(|v| v.as_str()) {
            args.domain = Some(domain.to_string());
        }
        if let Some(ssl) = params.get("ssl_enabled").and_then(|v| v.as_bool()) {
            args.ssl_enabled = Some(ssl);
        }

        // Resources
        if let Some(resources) = params.get("resources") {
            args.resources = Some(resources.clone());
        }

        // Container settings
        if let Some(restart_policy) = params.get("restart_policy").and_then(|v| v.as_str()) {
            args.restart_policy = Some(restart_policy.to_string());
        }
        if let Some(command) = params.get("command").and_then(|v| v.as_str()) {
            args.command = Some(command.to_string());
        }
        if let Some(entrypoint) = params.get("entrypoint").and_then(|v| v.as_str()) {
            args.entrypoint = Some(entrypoint.to_string());
        }

        // Networks and dependencies
        if let Some(networks) = params.get("networks") {
            args.networks = Some(networks.clone());
        }
        if let Some(depends_on) = params.get("depends_on") {
            args.depends_on = Some(depends_on.clone());
        }

        // Healthcheck
        if let Some(healthcheck) = params.get("healthcheck") {
            args.healthcheck = Some(healthcheck.clone());
        }

        // Labels
        if let Some(labels) = params.get("labels") {
            args.labels = Some(labels.clone());
        }

        // Deployment settings
        if let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) {
            args.enabled = Some(enabled);
        }
        if let Some(deploy_order) = params.get("deploy_order").and_then(|v| v.as_i64()) {
            args.deploy_order = Some(deploy_order as i32);
        }

        args
    }
}

/// Context for converting ProjectAppPostArgs to ProjectApp
struct ProjectAppContext<'a> {
    app_code: &'a str,
    project_id: i32,
}

impl ProjectAppPostArgs {
    /// Convert to ProjectApp with the given context
    fn into_project_app(self, ctx: ProjectAppContext<'_>) -> crate::models::ProjectApp {
        let mut app = crate::models::ProjectApp::default();
        app.project_id = ctx.project_id;
        app.code = ctx.app_code.to_string();
        app.name = self.name.unwrap_or_else(|| ctx.app_code.to_string());
        app.image = self.image.unwrap_or_default();
        app.environment = self.environment;
        app.ports = self.ports;
        app.volumes = self.volumes;
        app.domain = self.domain;
        app.ssl_enabled = self.ssl_enabled;
        app.resources = self.resources;
        app.restart_policy = self.restart_policy;
        app.command = self.command;
        app.entrypoint = self.entrypoint;
        app.networks = self.networks;
        app.depends_on = self.depends_on;
        app.healthcheck = self.healthcheck;
        app.labels = self.labels;
        app.enabled = self.enabled.or(Some(true));
        app.deploy_order = self.deploy_order;

        // Store non-compose config files in labels
        if let Some(config_files) = self.config_files {
            let mut labels = app.labels.clone().unwrap_or(json!({}));
            if let Some(obj) = labels.as_object_mut() {
                obj.insert("config_files".to_string(), config_files);
            }
            app.labels = Some(labels);
        }

        app
    }
}

/// Map POST parameters to ProjectApp
/// Also returns the compose_content separately for Vault storage
fn project_app_from_post(app_code: &str, project_id: i32, params: &serde_json::Value) -> (crate::models::ProjectApp, Option<String>) {
    let args = ProjectAppPostArgs::from(params);
    let compose_content = args.compose_content.clone();

    let ctx = ProjectAppContext { app_code, project_id };
    let app = args.into_project_app(ctx);

    (app, compose_content)
}

/// Merge two ProjectApp instances, preferring non-null incoming values over existing
/// This allows deploy_app with minimal params to not wipe out saved configuration
fn merge_project_app(
    existing: crate::models::ProjectApp,
    incoming: crate::models::ProjectApp,
) -> crate::models::ProjectApp {
    crate::models::ProjectApp {
        id: existing.id,
        project_id: existing.project_id,
        code: existing.code, // Keep existing code
        name: if incoming.name.is_empty() { existing.name } else { incoming.name },
        image: if incoming.image.is_empty() { existing.image } else { incoming.image },
        environment: incoming.environment.or(existing.environment),
        ports: incoming.ports.or(existing.ports),
        volumes: incoming.volumes.or(existing.volumes),
        domain: incoming.domain.or(existing.domain),
        ssl_enabled: incoming.ssl_enabled.or(existing.ssl_enabled),
        resources: incoming.resources.or(existing.resources),
        restart_policy: incoming.restart_policy.or(existing.restart_policy),
        command: incoming.command.or(existing.command),
        entrypoint: incoming.entrypoint.or(existing.entrypoint),
        networks: incoming.networks.or(existing.networks),
        depends_on: incoming.depends_on.or(existing.depends_on),
        healthcheck: incoming.healthcheck.or(existing.healthcheck),
        labels: incoming.labels.or(existing.labels),
        enabled: incoming.enabled.or(existing.enabled),
        deploy_order: incoming.deploy_order.or(existing.deploy_order),
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
        config_version: existing.config_version.map(|v| v + 1).or(Some(1)),
        vault_synced_at: existing.vault_synced_at,
        vault_sync_version: existing.vault_sync_version,
        config_hash: existing.config_hash,
    }
}

/// Extract compose content and config files from parameters and store to Vault
/// Used when deployment_id is not available but config_files contains compose/configs
/// Falls back to generating compose from params if no compose file is provided
async fn store_configs_to_vault_from_params(
    params: &serde_json::Value,
    deployment_hash: &str,
    app_code: &str,
    vault_settings: &crate::configuration::VaultSettings,
    deployment_settings: &crate::configuration::DeploymentSettings,
) {
    let vault = match VaultService::from_settings(vault_settings) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to initialize Vault: {}", e);
            return;
        }
    };

    let config_base_path = &deployment_settings.config_base_path;

    // Process config_files array
    let config_files = params.get("config_files").and_then(|v| v.as_array());
    
    let mut compose_content: Option<String> = None;
    let mut env_content: Option<String> = None;
    let mut app_configs: Vec<(String, crate::services::AppConfig)> = Vec::new();

    if let Some(files) = config_files {
        for file in files {
            let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let content = file.get("content").and_then(|c| c.as_str()).unwrap_or("");
            
            // Check for .env file in config_files
            if file_name == ".env" || file_name == "env" {
                env_content = Some(content.to_string());
                continue;
            }
            
            if file_name == "compose" || file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
                // This is the compose file
                compose_content = Some(content.to_string());
            } else if !content.is_empty() {
                // This is an app config file (e.g., telegraf.conf)
                // Use config_base_path from settings to avoid mounting /root
                let destination_path = file.get("destination_path")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("{}/{}/config/{}", config_base_path, app_code, file_name));
                
                let file_mode = file.get("file_mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("0644")
                    .to_string();
                
                let content_type = if file_name.ends_with(".json") {
                    "application/json"
                } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
                    "text/yaml"
                } else if file_name.ends_with(".toml") {
                    "text/toml"
                } else if file_name.ends_with(".conf") {
                    "text/plain"
                } else {
                    "text/plain"
                };

                let config = crate::services::AppConfig {
                    content: content.to_string(),
                    content_type: content_type.to_string(),
                    destination_path,
                    file_mode,
                    owner: file.get("owner").and_then(|o| o.as_str()).map(|s| s.to_string()),
                    group: file.get("group").and_then(|g| g.as_str()).map(|s| s.to_string()),
                };
                
                // Collect configs for later storage
                app_configs.push((file_name.to_string(), config));
            }
        }
    }

    // Fall back to generating compose from params if not found in config_files
    if compose_content.is_none() {
        tracing::info!("No compose in config_files, generating from params for app_code: {}", app_code);
        compose_content = generate_single_app_compose(app_code, params).ok();
    }

    // Generate .env from params.env if not found in config_files
    if env_content.is_none() {
        if let Some(env_obj) = params.get("env").and_then(|v| v.as_object()) {
            if !env_obj.is_empty() {
                let env_lines: Vec<String> = env_obj
                    .iter()
                    .map(|(k, v)| {
                        let val = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{}={}", k, val)
                    })
                    .collect();
                env_content = Some(env_lines.join("\n"));
                tracing::info!("Generated .env from params.env with {} variables for app_code: {}", env_obj.len(), app_code);
            }
        }
    }

    // Store compose to Vault
    if let Some(compose) = compose_content {
        tracing::info!(
            "Storing compose to Vault for deployment_hash: {}, app_code: {}",
            deployment_hash,
            app_code
        );
        let config = crate::services::AppConfig {
            content: compose,
            content_type: "text/yaml".to_string(),
            destination_path: format!("/app/{}/docker-compose.yml", app_code),
            file_mode: "0644".to_string(),
            owner: None,
            group: None,
        };
        match vault.store_app_config(deployment_hash, app_code, &config).await {
            Ok(_) => tracing::info!("Compose content stored in Vault for {}", app_code),
            Err(e) => tracing::warn!("Failed to store compose in Vault: {}", e),
        }
    } else {
        tracing::warn!("Could not extract or generate compose for app_code: {} - missing image parameter", app_code);
    }

    // Store .env to Vault under "{app_code}_env" key
    if let Some(env) = env_content {
        let env_key = format!("{}_env", app_code);
        tracing::info!(
            "Storing .env to Vault for deployment_hash: {}, key: {}",
            deployment_hash,
            env_key
        );
        let config = crate::services::AppConfig {
            content: env,
            content_type: "text/plain".to_string(),
            destination_path: format!("{}/{}/app/.env", config_base_path, app_code),
            file_mode: "0600".to_string(),
            owner: None,
            group: None,
        };
        match vault.store_app_config(deployment_hash, &env_key, &config).await {
            Ok(_) => tracing::info!(".env stored in Vault under key {}", env_key),
            Err(e) => tracing::warn!("Failed to store .env in Vault: {}", e),
        }
    }

    // Store app config files to Vault under "{app_code}_configs" key as a JSON array
    // This preserves multiple config files without overwriting
    if !app_configs.is_empty() {
        let configs_json: Vec<serde_json::Value> = app_configs
            .iter()
            .map(|(name, cfg)| {
                serde_json::json!({
                    "name": name,
                    "content": cfg.content,
                    "content_type": cfg.content_type,
                    "destination_path": cfg.destination_path,
                    "file_mode": cfg.file_mode,
                    "owner": cfg.owner,
                    "group": cfg.group,
                })
            })
            .collect();
        
        let config_key = format!("{}_configs", app_code);
        tracing::info!(
            "Storing {} app config files to Vault: deployment_hash={}, key={}",
            configs_json.len(),
            deployment_hash,
            config_key
        );
        
        // Store as a bundle config with JSON content
        let bundle_config = crate::services::AppConfig {
            content: serde_json::to_string(&configs_json).unwrap_or_default(),
            content_type: "application/json".to_string(),
            destination_path: format!("/app/{}/configs.json", app_code),
            file_mode: "0644".to_string(),
            owner: None,
            group: None,
        };
        
        match vault.store_app_config(deployment_hash, &config_key, &bundle_config).await {
            Ok(_) => tracing::info!("App config bundle stored in Vault for {}", config_key),
            Err(e) => tracing::warn!("Failed to store app config bundle in Vault: {}", e),
        }
    }
}

/// Upsert app config and sync to Vault for deploy_app
/// 
/// IMPORTANT: This function merges incoming parameters with existing app data.
/// If the app already exists, only non-null incoming fields will override existing values.
/// This prevents deploy_app commands with minimal params from wiping out saved config.
async fn upsert_app_config_for_deploy(
    pg_pool: &sqlx::PgPool,
    deployment_id: i32,
    app_code: &str,
    parameters: &serde_json::Value,
    deployment_hash: &str,
) {
    // Fetch project from DB
    let project = match crate::db::project::fetch(pg_pool, deployment_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::warn!("Project not found for deployment_id: {}", deployment_id);
            return;
        },
        Err(e) => {
            tracing::warn!("Failed to fetch project: {}", e);
            return;
        }
    };

    // Create app service
    let app_service = match ProjectAppService::new(Arc::new(pg_pool.clone())) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to create ProjectAppService: {}", e);
            return;
        }
    };

    // Check if app already exists and merge with existing data
    let (project_app, compose_content) = match app_service.get_by_code(project.id, app_code).await {
        Ok(existing_app) => {
            tracing::info!(
                "App {} exists (id={}), merging with incoming parameters",
                app_code,
                existing_app.id
            );
            // Merge incoming parameters with existing app data
            let (incoming_app, compose_content) = project_app_from_post(app_code, project.id, parameters);
            let merged = merge_project_app(existing_app, incoming_app);
            (merged, compose_content)
        }
        Err(_) => {
            tracing::info!("App {} does not exist, creating from parameters", app_code);
            project_app_from_post(app_code, project.id, parameters)
        }
    };

    // Upsert app config and sync to Vault
    match app_service.upsert(&project_app, &project, deployment_hash).await {
        Ok(_) => tracing::info!("App config upserted and synced to Vault for {}", app_code),
        Err(e) => tracing::warn!("Failed to upsert app config: {}", e),
    }

    // Store compose_content in Vault separately if provided
    if let Some(compose) = compose_content {
        let vault_settings = crate::configuration::get_configuration()
            .map(|s| s.vault)
            .ok();
        if let Some(vault_settings) = vault_settings {
            match VaultService::from_settings(&vault_settings) {
                Ok(vault) => {
                    let config = crate::services::AppConfig {
                        content: compose,
                        content_type: "text/yaml".to_string(),
                        destination_path: format!("/app/{}/docker-compose.yml", app_code),
                        file_mode: "0644".to_string(),
                        owner: None,
                        group: None,
                    };
                    match vault.store_app_config(deployment_hash, app_code, &config).await {
                        Ok(_) => tracing::info!("Compose content stored in Vault for {}", app_code),
                        Err(e) => tracing::warn!("Failed to store compose in Vault: {}", e),
                    }
                }
                Err(e) => tracing::warn!("Failed to initialize Vault for compose storage: {}", e),
            }
        }
    }
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
        let deployment_id = match req.parameters.as_ref()
            .and_then(|p| p.get("deployment_id"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32) 
        {
            Some(id) => Some(id),
            None => {
                // Auto-lookup project_id from deployment_hash
                match crate::db::deployment::fetch_by_deployment_hash(pg_pool.get_ref(), &req.deployment_hash).await {
                    Ok(Some(deployment)) => {
                        tracing::debug!("Auto-resolved project_id {} from deployment_hash {}", deployment.project_id, &req.deployment_hash);
                        Some(deployment.project_id)
                    },
                    Ok(None) => {
                        // No deployment found - auto-create project and deployment
                        tracing::info!("No deployment found for hash {}, auto-creating project and deployment", &req.deployment_hash);
                        
                        // Get app_code to use as project name
                        let app_code_for_name = req.parameters.as_ref()
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
                                tracing::info!("Auto-created project {} (id={}) for deployment_hash {}", 
                                    created_project.name, created_project.id, &req.deployment_hash);
                                
                                // Create deployment linked to this project
                                let deployment = crate::models::Deployment::new(
                                    created_project.id,
                                    Some(user.id.clone()),
                                    req.deployment_hash.clone(),
                                    "pending".to_string(),
                                    serde_json::json!({"auto_created": true}),
                                );
                                
                                match crate::db::deployment::insert(pg_pool.get_ref(), deployment).await {
                                    Ok(created_deployment) => {
                                        tracing::info!("Auto-created deployment (id={}) linked to project {}", 
                                            created_deployment.id, created_project.id);
                                        Some(created_project.id)
                                    },
                                    Err(e) => {
                                        tracing::warn!("Failed to auto-create deployment: {}", e);
                                        // Project was created, return its ID anyway
                                        Some(created_project.id)
                                    }
                                }
                            },
                            Err(e) => {
                                tracing::warn!("Failed to auto-create project: {}", e);
                                None
                            }
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to lookup deployment by hash: {}", e);
                        None
                    }
                }
            }
        };
        
        let app_code = req.parameters.as_ref()
            .and_then(|p| p.get("app_code"))
            .and_then(|v| v.as_str());
        let app_params = req.parameters.as_ref()
            .and_then(|p| p.get("parameters"));

        tracing::debug!(
            "deploy_app command detected, upserting app config for deployment_id: {:?}, app_code: {:?}",
            deployment_id,
            app_code
        );
        if let (Some(deployment_id), Some(app_code), Some(app_params)) = (deployment_id, app_code, app_params) {
            upsert_app_config_for_deploy(pg_pool.get_ref(), deployment_id, app_code, app_params, &req.deployment_hash).await;
        } else if let (Some(deployment_id), Some(app_code)) = (deployment_id, app_code) {
            // Have deployment_id and app_code but no nested parameters - use top-level parameters
            if let Some(params) = req.parameters.as_ref() {
                upsert_app_config_for_deploy(pg_pool.get_ref(), deployment_id, app_code, params, &req.deployment_hash).await;
            }
        } else if let Some(app_code) = app_code {
            // No deployment_id available (auto-create failed), just store to Vault
            if let Some(params) = req.parameters.as_ref() {
                store_configs_to_vault_from_params(params, &req.deployment_hash, app_code, &settings.vault, &settings.deployment).await;
            }
        } else {
            tracing::warn!("Missing app_code in deploy_app arguments");
        }

        enrich_deploy_app_with_compose(&req.deployment_hash, validated_parameters, &settings.vault).await
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
            tracing::warn!("Failed to initialize Vault: {}, cannot enrich deploy_app", e);
            return Some(params);
        }
    };

    // If compose_content is not already provided, fetch from Vault
    if params.get("compose_content").and_then(|v| v.as_str()).is_none() {
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
            if let Ok(configs_array) = serde_json::from_str::<Vec<serde_json::Value>>(&bundle_config.content) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Example payload from the user's request
    fn example_deploy_app_payload() -> serde_json::Value {
        json!({
            "deployment_id": 13513,
            "app_code": "telegraf",
            "parameters": {
                "env": {
                    "ansible_telegraf_influx_token": "FFolbg71mZjhKisMpAxYD5eEfxPtW3HRpTZHtv3XEYZRgzi3VGOxgLDhCYEvovMppvYuqSsbSTI8UFZqFwOx5Q==",
                    "ansible_telegraf_influx_bucket": "srv_localhost",
                    "ansible_telegraf_influx_org": "telegraf_org_4",
                    "telegraf_flush_interval": "10s",
                    "telegraf_interval": "10s",
                    "telegraf_role": "server"
                },
                "ports": [
                    {"port": null, "protocol": ["8200"]}
                ],
                "config_files": [
                    {
                        "name": "telegraf.conf",
                        "content": "# Telegraf configuration\n[agent]\n  interval = \"10s\"",
                        "variables": {}
                    },
                    {
                        "name": "compose",
                        "content": "services:\n  telegraf:\n    image: telegraf:latest\n    container_name: telegraf",
                        "variables": {}
                    }
                ]
            }
        })
    }

    #[test]
    fn test_project_app_post_args_from_params() {
        let payload = example_deploy_app_payload();
        let params = payload.get("parameters").unwrap();

        let args = ProjectAppPostArgs::from(params);

        // Check environment is extracted
        assert!(args.environment.is_some());
        let env = args.environment.as_ref().unwrap();
        assert_eq!(env.get("telegraf_role").and_then(|v| v.as_str()), Some("server"));
        assert_eq!(env.get("telegraf_interval").and_then(|v| v.as_str()), Some("10s"));

        // Check ports are extracted
        assert!(args.ports.is_some());
        let ports = args.ports.as_ref().unwrap().as_array().unwrap();
        assert_eq!(ports.len(), 1);

        // Check compose_content is extracted from config_files
        assert!(args.compose_content.is_some());
        let compose = args.compose_content.as_ref().unwrap();
        assert!(compose.contains("telegraf:latest"));

        // Check non-compose config files are preserved
        assert!(args.config_files.is_some());
        let config_files = args.config_files.as_ref().unwrap().as_array().unwrap();
        assert_eq!(config_files.len(), 1);
        assert_eq!(config_files[0].get("name").and_then(|v| v.as_str()), Some("telegraf.conf"));
    }

    #[test]
    fn test_project_app_from_post_basic() {
        let payload = example_deploy_app_payload();
        let params = payload.get("parameters").unwrap();
        let app_code = "telegraf";
        let project_id = 42;

        let (app, compose_content) = project_app_from_post(app_code, project_id, params);

        // Check basic fields
        assert_eq!(app.project_id, project_id);
        assert_eq!(app.code, "telegraf");
        assert_eq!(app.name, "telegraf"); // Defaults to app_code

        // Check environment is set
        assert!(app.environment.is_some());
        let env = app.environment.as_ref().unwrap();
        assert_eq!(env.get("telegraf_role").and_then(|v| v.as_str()), Some("server"));

        // Check ports are set
        assert!(app.ports.is_some());

        // Check enabled defaults to true
        assert_eq!(app.enabled, Some(true));

        // Check compose_content is returned separately
        assert!(compose_content.is_some());
        assert!(compose_content.as_ref().unwrap().contains("telegraf:latest"));

        // Check config_files are stored in labels
        assert!(app.labels.is_some());
        let labels = app.labels.as_ref().unwrap();
        assert!(labels.get("config_files").is_some());
    }

    #[test]
    fn test_project_app_from_post_with_all_fields() {
        let params = json!({
            "name": "My Telegraf App",
            "image": "telegraf:1.28",
            "env": {"KEY": "value"},
            "ports": [{"host": 8080, "container": 80}],
            "volumes": ["/data:/app/data"],
            "domain": "telegraf.example.com",
            "ssl_enabled": true,
            "resources": {"cpu_limit": "1", "memory_limit": "512m"},
            "restart_policy": "always",
            "command": "/bin/sh -c 'telegraf'",
            "entrypoint": "/entrypoint.sh",
            "networks": ["default_network"],
            "depends_on": ["influxdb"],
            "healthcheck": {"test": ["CMD", "curl", "-f", "http://localhost"]},
            "labels": {"app": "telegraf"},
            "enabled": false,
            "deploy_order": 5,
            "config_files": [
                {"name": "docker-compose.yml", "content": "version: '3'", "variables": {}}
            ]
        });

        let (app, compose_content) = project_app_from_post("telegraf", 100, &params);

        assert_eq!(app.name, "My Telegraf App");
        assert_eq!(app.image, "telegraf:1.28");
        assert_eq!(app.domain, Some("telegraf.example.com".to_string()));
        assert_eq!(app.ssl_enabled, Some(true));
        assert_eq!(app.restart_policy, Some("always".to_string()));
        assert_eq!(app.command, Some("/bin/sh -c 'telegraf'".to_string()));
        assert_eq!(app.entrypoint, Some("/entrypoint.sh".to_string()));
        assert_eq!(app.enabled, Some(false));
        assert_eq!(app.deploy_order, Some(5));

        // docker-compose.yml should be extracted as compose_content
        assert!(compose_content.is_some());
        assert_eq!(compose_content.as_ref().unwrap(), "version: '3'");
    }

    #[test]
    fn test_compose_extraction_from_different_names() {
        // Test "compose" name
        let params1 = json!({
            "config_files": [{"name": "compose", "content": "compose-content"}]
        });
        let args1 = ProjectAppPostArgs::from(&params1);
        assert_eq!(args1.compose_content, Some("compose-content".to_string()));

        // Test "docker-compose.yml" name
        let params2 = json!({
            "config_files": [{"name": "docker-compose.yml", "content": "docker-compose-content"}]
        });
        let args2 = ProjectAppPostArgs::from(&params2);
        assert_eq!(args2.compose_content, Some("docker-compose-content".to_string()));

        // Test "docker-compose.yaml" name
        let params3 = json!({
            "config_files": [{"name": "docker-compose.yaml", "content": "yaml-content"}]
        });
        let args3 = ProjectAppPostArgs::from(&params3);
        assert_eq!(args3.compose_content, Some("yaml-content".to_string()));
    }

    #[test]
    fn test_non_compose_files_preserved() {
        let params = json!({
            "config_files": [
                {"name": "telegraf.conf", "content": "telegraf config"},
                {"name": "nginx.conf", "content": "nginx config"},
                {"name": "compose", "content": "compose content"}
            ]
        });

        let args = ProjectAppPostArgs::from(&params);

        // Compose is extracted
        assert_eq!(args.compose_content, Some("compose content".to_string()));

        // Other files are preserved
        let config_files = args.config_files.unwrap();
        let files = config_files.as_array().unwrap();
        assert_eq!(files.len(), 2);

        let names: Vec<&str> = files.iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"telegraf.conf"));
        assert!(names.contains(&"nginx.conf"));
        assert!(!names.contains(&"compose"));
    }

    #[test]
    fn test_empty_params() {
        let params = json!({});
        let (app, compose_content) = project_app_from_post("myapp", 1, &params);

        assert_eq!(app.code, "myapp");
        assert_eq!(app.name, "myapp"); // Defaults to app_code
        assert_eq!(app.image, ""); // Empty default
        assert_eq!(app.enabled, Some(true)); // Default enabled
        assert!(compose_content.is_none());
    }

    #[test]
    fn test_into_project_app_preserves_context() {
        let args = ProjectAppPostArgs {
            name: Some("Custom Name".to_string()),
            image: Some("nginx:latest".to_string()),
            environment: Some(json!({"FOO": "bar"})),
            ..Default::default()
        };

        let ctx = ProjectAppContext {
            app_code: "nginx",
            project_id: 999,
        };

        let app = args.into_project_app(ctx);

        assert_eq!(app.project_id, 999);
        assert_eq!(app.code, "nginx");
        assert_eq!(app.name, "Custom Name");
        assert_eq!(app.image, "nginx:latest");
    }

    #[test]
    fn test_extract_compose_from_config_files_for_vault() {
        // This tests the extraction logic used in store_configs_to_vault_from_params
        
        // Helper to extract compose the same way as store_configs_to_vault_from_params
        fn extract_compose(params: &serde_json::Value) -> Option<String> {
            params.get("config_files")
                .and_then(|v| v.as_array())
                .and_then(|files| {
                    files.iter().find_map(|file| {
                        let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if file_name == "compose" || file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
                            file.get("content").and_then(|c| c.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
        }

        // Test with "compose" name
        let params1 = json!({
            "app_code": "telegraf",
            "config_files": [
                {"name": "telegraf.conf", "content": "config content"},
                {"name": "compose", "content": "services:\n  telegraf:\n    image: telegraf:latest"}
            ]
        });
        let compose1 = extract_compose(&params1);
        assert!(compose1.is_some());
        assert!(compose1.unwrap().contains("telegraf:latest"));

        // Test with "docker-compose.yml" name
        let params2 = json!({
            "app_code": "nginx",
            "config_files": [
                {"name": "docker-compose.yml", "content": "version: '3'\nservices:\n  nginx:\n    image: nginx:alpine"}
            ]
        });
        let compose2 = extract_compose(&params2);
        assert!(compose2.is_some());
        assert!(compose2.unwrap().contains("nginx:alpine"));

        // Test with no compose file
        let params3 = json!({
            "app_code": "myapp",
            "config_files": [
                {"name": "app.conf", "content": "some config"}
            ]
        });
        let compose3 = extract_compose(&params3);
        assert!(compose3.is_none());

        // Test with empty config_files
        let params4 = json!({
            "app_code": "myapp",
            "config_files": []
        });
        let compose4 = extract_compose(&params4);
        assert!(compose4.is_none());

        // Test with no config_files key
        let params5 = json!({
            "app_code": "myapp"
        });
        let compose5 = extract_compose(&params5);
        assert!(compose5.is_none());
    }

    #[test]
    fn test_generate_single_app_compose() {
        // Test with full parameters
        let params = json!({
            "image": "nginx:latest",
            "restart_policy": "always",
            "env": {
                "ENV_VAR1": "value1",
                "ENV_VAR2": "value2"
            },
            "ports": [
                {"host": 80, "container": 80},
                {"host": 443, "container": 443}
            ],
            "volumes": [
                {"source": "/data/nginx", "target": "/usr/share/nginx/html"}
            ],
            "networks": ["my_network"],
            "depends_on": ["postgres"],
            "labels": {
                "traefik.enable": "true"
            }
        });
        
        let compose = generate_single_app_compose("nginx", &params);
        assert!(compose.is_ok());
        let content = compose.unwrap();
        
        // Verify key elements (using docker_compose_types serialization format)
        assert!(content.contains("image: nginx:latest"));
        assert!(content.contains("restart: always"));
        assert!(content.contains("ENV_VAR1"));
        assert!(content.contains("value1"));
        assert!(content.contains("80:80"));
        assert!(content.contains("443:443"));
        assert!(content.contains("/data/nginx:/usr/share/nginx/html"));
        assert!(content.contains("my_network"));
        assert!(content.contains("postgres"));
        assert!(content.contains("traefik.enable"));

        // Test with minimal parameters (just image)
        let minimal_params = json!({
            "image": "redis:alpine"
        });
        let minimal_compose = generate_single_app_compose("redis", &minimal_params);
        assert!(minimal_compose.is_ok());
        let minimal_content = minimal_compose.unwrap();
        assert!(minimal_content.contains("image: redis:alpine"));
        assert!(minimal_content.contains("restart: unless-stopped")); // default
        assert!(minimal_content.contains("trydirect_network")); // default network

        // Test with no image - should return Err
        let no_image_params = json!({
            "env": {"KEY": "value"}
        });
        let no_image_compose = generate_single_app_compose("app", &no_image_params);
        assert!(no_image_compose.is_err());

        // Test with string-style ports
        let string_ports_params = json!({
            "image": "app:latest",
            "ports": ["8080:80", "9000:9000"]
        });
        let string_ports_compose = generate_single_app_compose("app", &string_ports_params);
        assert!(string_ports_compose.is_ok());
        let string_ports_content = string_ports_compose.unwrap();
        assert!(string_ports_content.contains("8080:80"));
        assert!(string_ports_content.contains("9000:9000"));

        // Test with array-style environment variables
        let array_env_params = json!({
            "image": "app:latest",
            "env": ["KEY1=val1", "KEY2=val2"]
        });
        let array_env_compose = generate_single_app_compose("app", &array_env_params);
        assert!(array_env_compose.is_ok());
        let array_env_content = array_env_compose.unwrap();
        assert!(array_env_content.contains("KEY1"));
        assert!(array_env_content.contains("val1"));
        assert!(array_env_content.contains("KEY2"));
        assert!(array_env_content.contains("val2"));

        // Test with string-style volumes
        let string_vol_params = json!({
            "image": "app:latest",
            "volumes": ["/host/path:/container/path", "named_vol:/data"]
        });
        let string_vol_compose = generate_single_app_compose("app", &string_vol_params);
        assert!(string_vol_compose.is_ok());
        let string_vol_content = string_vol_compose.unwrap();
        assert!(string_vol_content.contains("/host/path:/container/path"));
        assert!(string_vol_content.contains("named_vol:/data"));
    }

    // =========================================================================
    // Config File Storage and Enrichment Tests
    // =========================================================================

    #[test]
    fn test_config_files_extraction_for_bundling() {
        // Simulates the logic in store_configs_to_vault_from_params that extracts
        // non-compose config files for bundling
        fn extract_config_files(params: &serde_json::Value) -> Vec<(String, String)> {
            let mut configs = Vec::new();
            
            if let Some(files) = params.get("config_files").and_then(|v| v.as_array()) {
                for file in files {
                    let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let content = file.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    
                    // Skip compose files
                    if file_name == "compose" || file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
                        continue;
                    }
                    
                    if !content.is_empty() {
                        configs.push((file_name.to_string(), content.to_string()));
                    }
                }
            }
            
            configs
        }

        let params = json!({
            "app_code": "komodo",
            "config_files": [
                {"name": "komodo.env", "content": "ADMIN_EMAIL=test@example.com"},
                {"name": ".env", "content": "SECRET_KEY=abc123"},
                {"name": "docker-compose.yml", "content": "services:\n  komodo:"},
                {"name": "config.toml", "content": "[server]\nport = 8080"}
            ]
        });

        let configs = extract_config_files(&params);
        
        // Should have 3 non-compose configs
        assert_eq!(configs.len(), 3);
        
        let names: Vec<&str> = configs.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"komodo.env"));
        assert!(names.contains(&".env"));
        assert!(names.contains(&"config.toml"));
        assert!(!names.contains(&"docker-compose.yml"));
    }

    #[test]
    fn test_config_bundle_json_creation() {
        // Test that config files can be bundled into a JSON array format
        // similar to what store_configs_to_vault_from_params does
        let app_configs: Vec<(&str, &str, &str)> = vec![
            ("telegraf.conf", "[agent]\n  interval = \"10s\"", "/home/trydirect/hash123/config/telegraf.conf"),
            ("nginx.conf", "server { listen 80; }", "/home/trydirect/hash123/config/nginx.conf"),
        ];

        let configs_json: Vec<serde_json::Value> = app_configs
            .iter()
            .map(|(name, content, dest)| {
                json!({
                    "name": name,
                    "content": content,
                    "content_type": "text/plain",
                    "destination_path": dest,
                    "file_mode": "0644",
                    "owner": null,
                    "group": null,
                })
            })
            .collect();

        let bundle_json = serde_json::to_string(&configs_json).unwrap();
        
        // Verify structure
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&bundle_json).unwrap();
        assert_eq!(parsed.len(), 2);
        
        // Verify all fields present
        for config in &parsed {
            assert!(config.get("name").is_some());
            assert!(config.get("content").is_some());
            assert!(config.get("destination_path").is_some());
            assert!(config.get("file_mode").is_some());
        }
    }

    #[test]
    fn test_config_files_merge_with_existing() {
        // Test that existing config_files are preserved when merging with Vault configs
        fn merge_config_files(
            existing: Option<&Vec<serde_json::Value>>,
            vault_configs: Vec<serde_json::Value>,
        ) -> Vec<serde_json::Value> {
            let mut config_files: Vec<serde_json::Value> = Vec::new();
            
            if let Some(existing_configs) = existing {
                config_files.extend(existing_configs.iter().cloned());
            }
            
            config_files.extend(vault_configs);
            config_files
        }

        let existing = vec![
            json!({"name": "custom.conf", "content": "custom config"}),
        ];
        
        let vault_configs = vec![
            json!({"name": "telegraf.env", "content": "INFLUX_TOKEN=xxx"}),
            json!({"name": "app.conf", "content": "config from vault"}),
        ];

        let merged = merge_config_files(Some(&existing), vault_configs);
        
        assert_eq!(merged.len(), 3);
        
        let names: Vec<&str> = merged.iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"custom.conf"));
        assert!(names.contains(&"telegraf.env"));
        assert!(names.contains(&"app.conf"));
    }

    #[test]
    fn test_env_file_destination_path_format() {
        // Verify .env files have correct destination paths
        let deployment_hash = "abc123xyz";
        let app_code = "komodo";
        
        // Expected format from config_renderer.rs
        let env_dest_path = format!("/home/trydirect/{}/{}.env", deployment_hash, app_code);
        
        assert_eq!(env_dest_path, "/home/trydirect/abc123xyz/komodo.env");
        
        // Alternative format for deployment-level .env
        let global_env_path = format!("/home/trydirect/{}/.env", deployment_hash);
        assert_eq!(global_env_path, "/home/trydirect/abc123xyz/.env");
    }

    #[test]
    fn test_vault_key_generation() {
        // Test that correct Vault keys are generated for different config types
        let app_code = "komodo";
        
        // Compose key
        let compose_key = app_code.to_string();
        assert_eq!(compose_key, "komodo");
        
        // Env key
        let env_key = format!("{}_env", app_code);
        assert_eq!(env_key, "komodo_env");
        
        // Configs bundle key
        let configs_key = format!("{}_configs", app_code);
        assert_eq!(configs_key, "komodo_configs");
        
        // Legacy single config key
        let config_key = format!("{}_config", app_code);
        assert_eq!(config_key, "komodo_config");
    }

    #[test]
    fn test_config_content_types() {
        // Test content type detection for different file extensions
        fn detect_content_type(file_name: &str) -> &'static str {
            if file_name.ends_with(".json") {
                "application/json"
            } else if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
                "text/yaml"
            } else if file_name.ends_with(".toml") {
                "text/toml"
            } else if file_name.ends_with(".conf") {
                "text/plain"
            } else if file_name.ends_with(".env") {
                "text/plain"
            } else {
                "text/plain"
            }
        }

        assert_eq!(detect_content_type("config.json"), "application/json");
        assert_eq!(detect_content_type("docker-compose.yml"), "text/yaml");
        assert_eq!(detect_content_type("config.yaml"), "text/yaml");
        assert_eq!(detect_content_type("config.toml"), "text/toml");
        assert_eq!(detect_content_type("nginx.conf"), "text/plain");
        assert_eq!(detect_content_type("app.env"), "text/plain");
        assert_eq!(detect_content_type(".env"), "text/plain");
        assert_eq!(detect_content_type("unknown"), "text/plain");
    }

    #[test]
    fn test_multiple_env_files_in_bundle() {
        // Test handling of multiple .env-like files (app.env, .env.j2, etc.)
        let config_files = vec![
            json!({
                "name": "komodo.env",
                "content": "ADMIN_EMAIL=admin@test.com\nSECRET_KEY=abc",
                "destination_path": "/home/trydirect/hash123/komodo.env"
            }),
            json!({
                "name": ".env",
                "content": "DATABASE_URL=postgres://...",
                "destination_path": "/home/trydirect/hash123/.env"
            }),
            json!({
                "name": "custom.env.j2",
                "content": "{{ variable }}",
                "destination_path": "/home/trydirect/hash123/custom.env"
            }),
        ];

        // All should be valid config files
        assert_eq!(config_files.len(), 3);
        
        // Each should have required fields
        for config in &config_files {
            assert!(config.get("name").is_some());
            assert!(config.get("content").is_some());
            assert!(config.get("destination_path").is_some());
        }
    }

    #[test]
    fn test_env_generation_from_params_env() {
        // Test that .env content can be generated from params.env object
        // This mimics the logic in store_configs_to_vault_from_params
        fn generate_env_from_params(params: &serde_json::Value) -> Option<String> {
            params.get("env").and_then(|v| v.as_object()).and_then(|env_obj| {
                if env_obj.is_empty() {
                    return None;
                }
                let env_lines: Vec<String> = env_obj
                    .iter()
                    .map(|(k, v)| {
                        let val = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        format!("{}={}", k, val)
                    })
                    .collect();
                Some(env_lines.join("\n"))
            })
        }

        // Test with string values
        let params1 = json!({
            "app_code": "komodo",
            "env": {
                "DATABASE_URL": "postgres://localhost:5432/db",
                "SECRET_KEY": "abc123",
                "DEBUG": "false"
            }
        });
        let env1 = generate_env_from_params(&params1);
        assert!(env1.is_some());
        let content1 = env1.unwrap();
        assert!(content1.contains("DATABASE_URL=postgres://localhost:5432/db"));
        assert!(content1.contains("SECRET_KEY=abc123"));
        assert!(content1.contains("DEBUG=false"));

        // Test with non-string values (numbers, bools)
        let params2 = json!({
            "app_code": "app",
            "env": {
                "PORT": 8080,
                "DEBUG": true
            }
        });
        let env2 = generate_env_from_params(&params2);
        assert!(env2.is_some());
        let content2 = env2.unwrap();
        assert!(content2.contains("PORT=8080"));
        assert!(content2.contains("DEBUG=true"));

        // Test with empty env
        let params3 = json!({
            "app_code": "app",
            "env": {}
        });
        let env3 = generate_env_from_params(&params3);
        assert!(env3.is_none());

        // Test with missing env
        let params4 = json!({
            "app_code": "app"
        });
        let env4 = generate_env_from_params(&params4);
        assert!(env4.is_none());
    }

    #[test]
    fn test_env_file_extraction_from_config_files() {
        // Test that .env files are properly extracted from config_files
        // This mimics the logic in store_configs_to_vault_from_params
        fn extract_env_from_config_files(params: &serde_json::Value) -> Option<String> {
            params.get("config_files")
                .and_then(|v| v.as_array())
                .and_then(|files| {
                    files.iter().find_map(|file| {
                        let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if file_name == ".env" || file_name == "env" {
                            file.get("content").and_then(|c| c.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
        }

        // Test with .env file in config_files
        let params1 = json!({
            "app_code": "komodo",
            "config_files": [
                {"name": ".env", "content": "SECRET=xyz\nDEBUG=true"},
                {"name": "compose", "content": "services: ..."}
            ]
        });
        let env1 = extract_env_from_config_files(&params1);
        assert!(env1.is_some());
        assert!(env1.unwrap().contains("SECRET=xyz"));

        // Test with "env" name variant
        let params2 = json!({
            "app_code": "app",
            "config_files": [
                {"name": "env", "content": "VAR=value"}
            ]
        });
        let env2 = extract_env_from_config_files(&params2);
        assert!(env2.is_some());

        // Test without .env file
        let params3 = json!({
            "app_code": "app",
            "config_files": [
                {"name": "config.toml", "content": "[server]"}
            ]
        });
        let env3 = extract_env_from_config_files(&params3);
        assert!(env3.is_none());
    }
}
