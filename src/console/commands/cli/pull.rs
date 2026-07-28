use crate::cli::config_parser::{AppType, ConfigBuilder, DeployTarget, ServiceDefinition};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{DeploymentStatusInfo, ProjectAppInfo, ProjectInfo};
use crate::console::commands::cli::init::serialize_generated_config;
use crate::console::commands::CallableTrait;
use dialoguer::Confirm;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

pub struct PullCommand {
    pub project_ref: String,
    pub force: bool,
    pub dir: Option<PathBuf>,
    pub json: bool,
}

impl PullCommand {
    pub fn new(project_ref: String, force: bool, dir: Option<PathBuf>, json: bool) -> Self {
        Self {
            project_ref,
            force,
            dir,
            json,
        }
    }
}

impl CallableTrait for PullCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pull")?;
        let project_dir = self
            .dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap());
        let force = self.force;
        let json = self.json;
        let project_ref = self.project_ref.clone();

        ctx.block_on(async {
            // 1. Resolve project by ID or name
            let project = ctx
                .client
                .find_project(&project_ref)
                .await?
                .ok_or_else(|| {
                    CliError::ConfigValidation(format!("Project '{}' not found", project_ref))
                })?;

            eprintln!("Pulling project '{}' (ID: {})...", project.name, project.id);

            // 2. Fetch apps
            let apps = ctx.client.list_project_apps(project.id).await?;

            // 3. Fetch latest deployment (may not exist)
            let deployment = ctx
                .client
                .get_deployment_status_by_project(project.id)
                .await?;

            // 4. Determine deploy target
            let target = if deployment.is_some() {
                detect_deploy_target(&ctx, project.id).await
            } else {
                DeployTarget::Cloud
            };

            // 5. Build stacker.yml content
            let stacker_yml = if let Some(ref dep) = deployment {
                build_stacker_yml_from_deployment(&project, &apps, dep, target)
            } else {
                build_stacker_yml_from_template(&project)?
            };

            // 6. Check for existing files and ask confirmation
            let config_path = project_dir.join("stacker.yml");
            if config_path.exists() && !force {
                if !io::stdin().is_terminal() {
                    return Err(CliError::ConfigValidation(
                        "stacker.yml already exists. Use --force to overwrite.".to_string(),
                    )
                    .into());
                }
                let overwrite = Confirm::new()
                    .with_prompt(format!(
                        "stacker.yml already exists in {}. Overwrite?",
                        project_dir.display()
                    ))
                    .default(false)
                    .interact()?;
                if !overwrite {
                    eprintln!("Aborted.");
                    return Ok(());
                }
            }

            let lock_path = DeploymentLock::lockfile_path_for_target(
                &project_dir,
                &format!("{:?}", target).to_lowercase(),
            );
            if lock_path.exists() && !force {
                if !io::stdin().is_terminal() {
                    return Err(CliError::ConfigValidation(
                        "Deployment lock already exists. Use --force to overwrite.".to_string(),
                    )
                    .into());
                }
                let overwrite = Confirm::new()
                    .with_prompt("Deployment lock already exists. Overwrite?")
                    .default(false)
                    .interact()?;
                if !overwrite {
                    eprintln!("Aborted.");
                    return Ok(());
                }
            }

            // 7. Hydrate directory
            hydrate_project_dir(&project_dir, &stacker_yml, &project, &deployment, target)?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "project_id": project.id,
                        "project_name": project.name,
                        "deployment_hash": deployment.as_ref().map(|d| &d.deployment_hash),
                        "target": format!("{:?}", target).to_lowercase(),
                        "stacker_yml": stacker_yml,
                    }))?
                );
            }

            Ok(())
        })
    }
}

/// Detect deploy target by checking if a server exists for the project.
/// Uses the same logic as the handoff route: if server has cloud_id -> cloud,
/// if server exists without cloud_id -> server, otherwise -> cloud (default).
async fn detect_deploy_target(ctx: &CliRuntime, project_id: i32) -> DeployTarget {
    match ctx.client.list_servers().await {
        Ok(servers) => {
            let project_server = servers.iter().find(|s| s.project_id == project_id);
            match project_server {
                Some(srv) if srv.cloud_id.is_some() => DeployTarget::Cloud,
                Some(_) => DeployTarget::Server,
                None => DeployTarget::Cloud,
            }
        }
        Err(_) => DeployTarget::Cloud,
    }
}

/// Build stacker.yml from a deployed project's apps and deployment.
fn build_stacker_yml_from_deployment(
    project: &ProjectInfo,
    apps: &[ProjectAppInfo],
    _deployment: &DeploymentStatusInfo,
    target: DeployTarget,
) -> String {
    let mut builder = ConfigBuilder::new()
        .name(&project.name)
        .project_identity(&project.name)
        .deploy_target(target);

    // Primary app (no parent)
    if let Some(primary) = apps.iter().find(|a| a.parent_app_code.is_none()) {
        builder = builder
            .app_type(infer_app_type(&primary.image))
            .app_image(&primary.image);
    }

    // Services (have parent)
    for app in apps.iter().filter(|a| a.parent_app_code.is_some()) {
        builder = builder.add_service(ServiceDefinition {
            name: app.code.clone(),
            image: app.image.clone(),
            ports: Vec::new(),
            environment: std::collections::HashMap::new(),
            volumes: Vec::new(),
            depends_on: Vec::new(),
            command: None,
            healthcheck: None,
        });
    }

    let config = builder.build().expect("Failed to build config");
    serialize_generated_config(&config).expect("Failed to serialize config")
}

/// Build stacker.yml from project template metadata (no deployment case).
/// Tries to extract from marketplace_config_files, falls back to minimal config.
fn build_stacker_yml_from_template(
    project: &ProjectInfo,
) -> Result<String, Box<dyn std::error::Error>> {
    let metadata = &project.metadata;

    // Try to extract stacker.yml from marketplace_config_files
    if let Some(config_files) = metadata.pointer("/custom/marketplace_config_files") {
        if let Some(files) = config_files.as_array() {
            for file in files {
                if let (Some(name), Some(content)) = (
                    file.get("name").and_then(|n| n.as_str()),
                    file.get("content").and_then(|c| c.as_str()),
                ) {
                    if name == "stacker.yml" || name == "stacker.yaml" {
                        return Ok(content.to_string());
                    }
                }
            }
        }
    }

    // Fallback: generate minimal config
    let builder = ConfigBuilder::new()
        .name(&project.name)
        .project_identity(&project.name);

    let config = builder.build()?;
    Ok(serialize_generated_config(&config)?)
}

/// Infer app type from Docker image name.
fn infer_app_type(image: &str) -> AppType {
    let lower = image.to_lowercase();
    if lower.contains("node") || lower.contains("npm") {
        AppType::Node
    } else if lower.contains("python") || lower.contains("flask") || lower.contains("django") {
        AppType::Python
    } else if lower.contains("rust") || lower.contains("actix") {
        AppType::Rust
    } else if lower.contains("go") || lower.contains("golang") {
        AppType::Go
    } else if lower.contains("php") || lower.contains("fpm") {
        AppType::Php
    } else if lower.contains("nginx") || lower.contains("httpd") || lower.contains("apache") {
        AppType::Static
    } else {
        AppType::Custom
    }
}

/// Hydrate the local project directory with stacker.yml and deployment lock.
fn hydrate_project_dir(
    project_dir: &Path,
    stacker_yml: &str,
    project: &ProjectInfo,
    deployment: &Option<DeploymentStatusInfo>,
    target: DeployTarget,
) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure directory exists
    std::fs::create_dir_all(project_dir)?;

    // Write stacker.yml
    let config_path = project_dir.join("stacker.yml");
    std::fs::write(&config_path, stacker_yml)?;

    // Create deployment lock if deployment exists
    if let Some(dep) = deployment {
        let lock = DeploymentLock {
            target: format!("{:?}", target).to_lowercase(),
            server_ip: None,
            ssh_user: None,
            ssh_port: None,
            ssh_key: None,
            server_name: None,
            deployment_id: Some(dep.id as i64),
            project_id: Some(project.id as i64),
            cloud_id: None,
            project_name: Some(project.name.clone()),
            stacker_email: None,
            deployed_at: dep.created_at.clone(),
        };
        lock.save(project_dir)?;
    }

    eprintln!("✓ Pulled project '{}' (ID: {})", project.name, project.id);
    eprintln!("  stacker.yml: {}", config_path.display());
    if let Some(dep) = deployment {
        eprintln!("  Deployment: {}", dep.deployment_hash);
    } else {
        eprintln!("  No deployment yet — run `stacker deploy` to create one");
    }
    eprintln!();
    eprintln!("  Next steps:");
    eprintln!("    stacker deploy");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_app_type_node() {
        assert_eq!(infer_app_type("node:20-alpine"), AppType::Node);
        assert_eq!(infer_app_type("myorg/npm-builder"), AppType::Node);
    }

    #[test]
    fn infer_app_type_python() {
        assert_eq!(infer_app_type("python:3.12-slim"), AppType::Python);
        assert_eq!(infer_app_type("flask-app"), AppType::Python);
        assert_eq!(infer_app_type("django:latest"), AppType::Python);
    }

    #[test]
    fn infer_app_type_rust() {
        assert_eq!(infer_app_type("rust:1.77-alpine"), AppType::Rust);
        assert_eq!(infer_app_type("actix-web"), AppType::Rust);
    }

    #[test]
    fn infer_app_type_go() {
        assert_eq!(infer_app_type("golang:1.22-alpine"), AppType::Go);
        assert_eq!(infer_app_type("myapp-go"), AppType::Go);
    }

    #[test]
    fn infer_app_type_php() {
        assert_eq!(infer_app_type("php:8.3-fpm-alpine"), AppType::Php);
        assert_eq!(infer_app_type("fpm-app"), AppType::Php);
    }

    #[test]
    fn infer_app_type_static() {
        assert_eq!(infer_app_type("nginx:alpine"), AppType::Static);
        assert_eq!(infer_app_type("httpd:latest"), AppType::Static);
        assert_eq!(infer_app_type("apache:2.4"), AppType::Static);
    }

    #[test]
    fn infer_app_type_custom() {
        assert_eq!(infer_app_type("redis:7"), AppType::Custom);
        assert_eq!(infer_app_type("postgres:16"), AppType::Custom);
        assert_eq!(infer_app_type("mailu/core"), AppType::Custom);
    }

    #[test]
    fn build_stacker_yml_from_deployment_generates_config() {
        let project = ProjectInfo {
            id: 42,
            name: "my-project".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({}),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-01".to_string(),
        };

        let apps = vec![
            ProjectAppInfo {
                id: 1,
                project_id: 42,
                code: "web".to_string(),
                name: "Web".to_string(),
                image: "node:20-alpine".to_string(),
                enabled: true,
                deploy_order: Some(1),
                parent_app_code: None,
            },
            ProjectAppInfo {
                id: 2,
                project_id: 42,
                code: "db".to_string(),
                name: "Database".to_string(),
                image: "postgres:16".to_string(),
                enabled: true,
                deploy_order: Some(2),
                parent_app_code: Some("web".to_string()),
            },
        ];

        let deployment = DeploymentStatusInfo {
            id: 100,
            project_id: 42,
            deployment_hash: "deployment_abc123".to_string(),
            status: "completed".to_string(),
            status_message: None,
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-01".to_string(),
        };

        let yml =
            build_stacker_yml_from_deployment(&project, &apps, &deployment, DeployTarget::Cloud);

        assert!(yml.contains("name: my-project"));
        assert!(yml.contains("identity: my-project"));
        assert!(yml.contains("image: node:20-alpine"));
        assert!(yml.contains("name: db"));
        assert!(yml.contains("image: postgres:16"));
    }

    #[test]
    fn build_stacker_yml_from_template_with_no_metadata() {
        let project = ProjectInfo {
            id: 42,
            name: "my-project".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({}),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-01".to_string(),
        };

        let yml = build_stacker_yml_from_template(&project).unwrap();
        assert!(yml.contains("name: my-project"));
    }

    #[test]
    fn build_stacker_yml_from_template_with_marketplace_config() {
        let project = ProjectInfo {
            id: 42,
            name: "mailu".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({
                "custom": {
                    "marketplace_config_files": [
                        {
                            "name": "docker-compose.yml",
                            "content": "version: '3'\nservices:\n  app:\n    image: mailu/core"
                        },
                        {
                            "name": "stacker.yml",
                            "content": "name: mailu\napp:\n  type: custom\n  image: mailu/core"
                        }
                    ]
                }
            }),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-01".to_string(),
        };

        let yml = build_stacker_yml_from_template(&project).unwrap();
        assert!(yml.contains("name: mailu"));
        assert!(yml.contains("image: mailu/core"));
    }
}
