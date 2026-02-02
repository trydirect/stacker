use crate::configuration::{DeploymentSettings, VaultSettings};
use crate::helpers::project::builder::generate_single_app_compose;
use crate::services::{AppConfig, VaultService};

/// Extract compose content and config files from parameters and store to Vault
/// Used when deployment_id is not available but config_files contains compose/configs
/// Falls back to generating compose from params if no compose file is provided
pub(crate) async fn store_configs_to_vault_from_params(
    params: &serde_json::Value,
    deployment_hash: &str,
    app_code: &str,
    vault_settings: &VaultSettings,
    deployment_settings: &DeploymentSettings,
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
    let mut app_configs: Vec<(String, AppConfig)> = Vec::new();

    if let Some(files) = config_files {
        for file in files {
            let file_name = file.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let content = file.get("content").and_then(|c| c.as_str()).unwrap_or("");

            // Check for .env file in config_files
            if is_env_filename(file_name) {
                env_content = Some(content.to_string());
                continue;
            }

            if super::is_compose_filename(file_name) {
                // This is the compose file
                compose_content = Some(content.to_string());
            } else if !content.is_empty() {
                // This is an app config file (e.g., telegraf.conf)
                // Use config_base_path from settings to avoid mounting /root
                let destination_path = file
                    .get("destination_path")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        format!("{}/{}/config/{}", config_base_path, app_code, file_name)
                    });

                let file_mode = file
                    .get("file_mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("0644")
                    .to_string();

                let content_type = detect_content_type(file_name).to_string();

                let config = AppConfig {
                    content: content.to_string(),
                    content_type,
                    destination_path,
                    file_mode,
                    owner: file
                        .get("owner")
                        .and_then(|o| o.as_str())
                        .map(|s| s.to_string()),
                    group: file
                        .get("group")
                        .and_then(|g| g.as_str())
                        .map(|s| s.to_string()),
                };

                // Collect configs for later storage
                app_configs.push((file_name.to_string(), config));
            }
        }
    }

    // Fall back to generating compose from params if not found in config_files
    if compose_content.is_none() {
        tracing::info!(
            "No compose in config_files, generating from params for app_code: {}",
            app_code
        );
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
                tracing::info!(
                    "Generated .env from params.env with {} variables for app_code: {}",
                    env_obj.len(),
                    app_code
                );
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
        let config = AppConfig {
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
    } else {
        tracing::warn!(
            "Could not extract or generate compose for app_code: {} - missing image parameter",
            app_code
        );
    }

    // Store .env to Vault under "{app_code}_env" key
    if let Some(env) = env_content {
        let env_key = format!("{}_env", app_code);
        tracing::info!(
            "Storing .env to Vault for deployment_hash: {}, key: {}",
            deployment_hash,
            env_key
        );
        let config = AppConfig {
            content: env,
            content_type: "text/plain".to_string(),
            // Path must match docker-compose env_file: "/home/trydirect/{app_code}/.env"
            destination_path: format!("{}/{}/.env", config_base_path, app_code),
            file_mode: "0600".to_string(),
            owner: None,
            group: None,
        };
        match vault
            .store_app_config(deployment_hash, &env_key, &config)
            .await
        {
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
        let bundle_config = AppConfig {
            content: serde_json::to_string(&configs_json).unwrap_or_default(),
            content_type: "application/json".to_string(),
            destination_path: format!("/app/{}/configs.json", app_code),
            file_mode: "0644".to_string(),
            owner: None,
            group: None,
        };

        match vault
            .store_app_config(deployment_hash, &config_key, &bundle_config)
            .await
        {
            Ok(_) => tracing::info!("App config bundle stored in Vault for {}", config_key),
            Err(e) => tracing::warn!("Failed to store app config bundle in Vault: {}", e),
        }
    }
}

fn is_env_filename(file_name: &str) -> bool {
    matches!(file_name, ".env" | "env")
}

pub(crate) fn detect_content_type(file_name: &str) -> &'static str {
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
