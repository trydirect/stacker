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
            let file_name = get_str(file, "name").unwrap_or("");
            let content = get_str(file, "content").unwrap_or("");

            if is_env_filename(file_name) {
                env_content = Some(content.to_string());
                continue;
            }

            if content.is_empty() {
                continue;
            }

            let content_type = get_str(file, "content_type")
                .map(|s| s.to_string())
                .unwrap_or_else(|| detect_content_type(file_name).to_string());

            if is_compose_file(file_name, &content_type) {
                compose_content = Some(content.to_string());

                let compose_filename = normalize_compose_filename(file_name);
                let destination_path = resolve_destination_path(
                    file,
                    format!("{}/{}/{}", config_base_path, app_code, compose_filename),
                );

                let compose_type = if content_type == "text/plain" {
                    "text/yaml".to_string()
                } else {
                    content_type
                };

                let config =
                    build_app_config(content, compose_type, destination_path, file, "0644");

                app_configs.push((compose_filename, config));
                continue;
            }

            let destination_path = resolve_destination_path(
                file,
                format!("{}/{}/{}", config_base_path, app_code, file_name),
            );
            let config = build_app_config(content, content_type, destination_path, file, "0644");

            app_configs.push((file_name.to_string(), config));
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

    // Store compose to Vault with correct destination path
    if let Some(compose) = compose_content {
        tracing::info!(
            "Storing compose to Vault for deployment_hash: {}, app_code: {}",
            deployment_hash,
            app_code
        );
        let config = AppConfig {
            content: compose,
            content_type: "text/yaml".to_string(),
            // Use config_base_path for consistent deployment root path
            destination_path: format!("{}/{}/docker-compose.yml", config_base_path, app_code),
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

fn is_compose_file(file_name: &str, content_type: &str) -> bool {
    if super::is_compose_filename(file_name) {
        return true;
    }

    content_type == "text/yaml" && matches!(file_name, "docker-compose" | "compose")
}

fn normalize_compose_filename(file_name: &str) -> String {
    if file_name.ends_with(".yml") || file_name.ends_with(".yaml") {
        return file_name.to_string();
    }

    format!("{}.yml", file_name)
}

fn resolve_destination_path(file: &serde_json::Value, default_path: String) -> String {
    get_str(file, "destination_path")
        .map(|s| s.to_string())
        .unwrap_or(default_path)
}

fn build_app_config(
    content: &str,
    content_type: String,
    destination_path: String,
    file: &serde_json::Value,
    default_mode: &str,
) -> AppConfig {
    let file_mode = get_str(file, "file_mode")
        .unwrap_or(default_mode)
        .to_string();

    AppConfig {
        content: content.to_string(),
        content_type,
        destination_path,
        file_mode,
        owner: get_str(file, "owner").map(|s| s.to_string()),
        group: get_str(file, "group").map(|s| s.to_string()),
    }
}

fn get_str<'a>(file: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    file.get(key).and_then(|v| v.as_str())
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
