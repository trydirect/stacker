use std::sync::Arc;

use crate::services::{ProjectAppService, VaultService};

use super::{merge_project_app, project_app_from_post, store_configs_to_vault_from_params};

/// Upsert app config and sync to Vault for deploy_app
///
/// IMPORTANT: This function merges incoming parameters with existing app data.
/// If the app already exists, only non-null incoming fields will override existing values.
/// This prevents deploy_app commands with minimal params from wiping out saved config.
pub(crate) async fn upsert_app_config_for_deploy(
    pg_pool: &sqlx::PgPool,
    deployment_id: i32,
    app_code: &str,
    parameters: &serde_json::Value,
    deployment_hash: &str,
) {
    tracing::info!(
        "[UPSERT_APP_CONFIG] START - deployment_id: {}, app_code: {}, deployment_hash: {}",
        deployment_id,
        app_code,
        deployment_hash
    );
    tracing::info!(
        "[UPSERT_APP_CONFIG] Parameters: {}",
        serde_json::to_string_pretty(parameters).unwrap_or_else(|_| parameters.to_string())
    );

    // Fetch project from DB
    let project = match crate::db::project::fetch(pg_pool, deployment_id).await {
        Ok(Some(p)) => {
            tracing::info!(
                "[UPSERT_APP_CONFIG] Found project id={}, name={}",
                p.id,
                p.name
            );
            p
        }
        Ok(None) => {
            tracing::warn!(
                "[UPSERT_APP_CONFIG] Project not found for deployment_id: {}",
                deployment_id
            );
            return;
        }
        Err(e) => {
            tracing::warn!("[UPSERT_APP_CONFIG] Failed to fetch project: {}", e);
            return;
        }
    };

    // Create app service
    let app_service = match ProjectAppService::new(Arc::new(pg_pool.clone())) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "[UPSERT_APP_CONFIG] Failed to create ProjectAppService: {}",
                e
            );
            return;
        }
    };

    // Check if app already exists and merge with existing data
    let (project_app, compose_content) = match app_service.get_by_code(project.id, app_code).await {
        Ok(existing_app) => {
            tracing::info!(
                "[UPSERT_APP_CONFIG] App {} exists (id={}, image={}), merging with incoming parameters",
                app_code,
                existing_app.id,
                existing_app.image
            );
            // Merge incoming parameters with existing app data
            let (incoming_app, compose_content) =
                project_app_from_post(app_code, project.id, parameters);
            tracing::info!(
                "[UPSERT_APP_CONFIG] Incoming app parsed - image: {}, env: {:?}",
                incoming_app.image,
                incoming_app.environment
            );
            let merged = merge_project_app(existing_app, incoming_app);
            tracing::info!(
                "[UPSERT_APP_CONFIG] Merged app - image: {}, env: {:?}",
                merged.image,
                merged.environment
            );
            (merged, compose_content)
        }
        Err(e) => {
            tracing::info!(
                "[UPSERT_APP_CONFIG] App {} does not exist ({}), creating from parameters",
                app_code,
                e
            );
            let (new_app, compose_content) =
                project_app_from_post(app_code, project.id, parameters);
            tracing::info!(
                "[UPSERT_APP_CONFIG] New app parsed - image: {}, env: {:?}, compose_content: {}",
                new_app.image,
                new_app.environment,
                compose_content.is_some()
            );
            (new_app, compose_content)
        }
    };

    // Log final project_app before upsert
    tracing::info!(
        "[UPSERT_APP_CONFIG] Final project_app - code: {}, name: {}, image: {}, env: {:?}",
        project_app.code,
        project_app.name,
        project_app.image,
        project_app.environment
    );

    // Upsert app config and sync to Vault
    match app_service
        .upsert(&project_app, &project, deployment_hash)
        .await
    {
        Ok(saved) => tracing::info!(
            "[UPSERT_APP_CONFIG] SUCCESS - App {} saved with id={}, synced to Vault",
            app_code,
            saved.id
        ),
        Err(e) => tracing::error!(
            "[UPSERT_APP_CONFIG] FAILED to upsert app {}: {}",
            app_code,
            e
        ),
    }

    // If config files or env were provided in parameters, ensure they are stored to Vault
    // This captures raw .env content from config_files for Status Panel deploys.
    if parameters.get("config_files").is_some() || parameters.get("env").is_some() {
        if let Ok(settings) = crate::configuration::get_configuration() {
            store_configs_to_vault_from_params(
                parameters,
                deployment_hash,
                app_code,
                &settings.vault,
                &settings.deployment,
            )
            .await;
        } else {
            tracing::warn!("Failed to load configuration for Vault config storage");
        }
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
                    match vault
                        .store_app_config(deployment_hash, app_code, &config)
                        .await
                    {
                        Ok(_) => tracing::info!("Compose content stored in Vault for {}", app_code),
                        Err(e) => tracing::warn!("Failed to store compose in Vault: {}", e),
                    }
                }
                Err(e) => tracing::warn!("Failed to initialize Vault for compose storage: {}", e),
            }
        }
    }
}
