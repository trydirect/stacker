//! Stacker Server API Client for CLI
//!
//! Communicates with the Stacker server (not User Service directly) for:
//! - Project CRUD (list, create, lookup by name)
//! - Cloud credential management (list, lookup by provider)
//! - Server management (list, lookup by name)
//! - Deployment (POST /project/{id}/deploy or /project/{id}/deploy/{cloud_id})
//!
//! All endpoints require `Authorization: Bearer <token>` from `stacker login`.

use crate::cli::error::CliError;
use serde::{Deserialize, Serialize};

/// Default Stacker server base URL (distinct from the User Service auth URL).
pub const DEFAULT_STACKER_URL: &str = "https://stacker.try.direct";

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Response types (matching Stacker server JSON envelope)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Stacker server wraps responses in `{ "item": ..., "list": [...], "msg": "...", "_status": "OK" }`
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    #[serde(rename = "_status")]
    pub status: Option<String>,
    pub msg: Option<String>,
    pub item: Option<T>,
    pub list: Option<Vec<T>>,
    pub id: Option<i32>,
    pub meta: Option<serde_json::Value>,
}

/// Project as returned by `/project` endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: i32,
    pub name: String,
    pub user_id: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Cloud credentials as returned by `/cloud` endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInfo {
    pub id: i32,
    pub user_id: String,
    pub provider: String,
    pub cloud_token: Option<String>,
    pub cloud_key: Option<String>,
    pub cloud_secret: Option<String>,
    pub save_token: Option<bool>,
}

/// Server as returned by `/server` endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub id: i32,
    pub user_id: String,
    pub project_id: i32,
    pub cloud_id: Option<i32>,
    #[serde(default)]
    pub cloud: Option<String>,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub server: Option<String>,
    pub os: Option<String>,
    pub disk_type: Option<String>,
    pub srv_ip: Option<String>,
    pub ssh_port: Option<i32>,
    pub ssh_user: Option<String>,
    pub name: Option<String>,
    #[serde(default = "default_connection_mode")]
    pub connection_mode: String,
    #[serde(default = "default_key_status")]
    pub key_status: String,
}

fn default_connection_mode() -> String {
    "ssh".to_string()
}
fn default_key_status() -> String {
    "none".to_string()
}

/// Deploy response from `/project/{id}/deploy`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    pub id: Option<i32>,
    #[serde(rename = "_status")]
    pub status: Option<String>,
    pub msg: Option<String>,
    pub meta: Option<serde_json::Value>,
}

/// Deployment status info from `/api/v1/deployments/{id}`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStatusInfo {
    pub id: i32,
    pub project_id: i32,
    pub deployment_hash: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// StackerClient — HTTP client for the Stacker server
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct StackerClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl StackerClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http,
        }
    }

    // ── Projects ─────────────────────────────────────

    /// List all projects for the authenticated user.
    pub async fn list_projects(&self) -> Result<Vec<ProjectInfo>, CliError> {
        let url = format!("{}/project", self.base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server GET /project failed ({}): {}", status, body),
            });
        }

        let api: ApiResponse<ProjectInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        Ok(api.list.unwrap_or_default())
    }

    /// Find a project by name (case-insensitive).
    pub async fn find_project_by_name(&self, name: &str) -> Result<Option<ProjectInfo>, CliError> {
        let projects = self.list_projects().await?;
        let lower = name.to_lowercase();
        Ok(projects
            .into_iter()
            .find(|p| p.name.to_lowercase() == lower))
    }

    /// Create a project on the Stacker server.
    pub async fn create_project(
        &self,
        name: &str,
        metadata: serde_json::Value,
    ) -> Result<ProjectInfo, CliError> {
        let url = format!("{}/project", self.base_url);

        // If metadata already has "custom" key (e.g. from build_project_body),
        // use it directly. Otherwise, wrap in a default structure.
        let body = if metadata.get("custom").is_some() {
            // Ensure custom_stack_code is set to the project name
            let mut body = metadata;
            if let Some(custom) = body.get_mut("custom").and_then(|c| c.as_object_mut()) {
                custom
                    .entry("custom_stack_code")
                    .or_insert_with(|| serde_json::json!(name));
            }
            body
        } else {
            let payload = serde_json::json!({
                "custom": {
                    "custom_stack_code": name,
                    "web": [],
                    "feature": [],
                    "service": [],
                }
            });

            // Merge metadata if provided
            if metadata.is_object() {
                let mut base = payload;
                if let Some(obj) = base.as_object_mut() {
                    if let Some(meta_obj) = metadata.as_object() {
                        for (k, v) in meta_obj {
                            obj.insert(k.clone(), v.clone());
                        }
                    }
                }
                base
            } else {
                payload
            }
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server POST /project failed ({}): {}",
                    status, body
                ),
            });
        }

        let api: ApiResponse<ProjectInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        api.item.ok_or_else(|| CliError::DeployFailed {
            target: crate::cli::config_parser::DeployTarget::Cloud,
            reason: "Stacker server created project but returned no item".to_string(),
        })
    }

    /// Update an existing project's metadata on the Stacker server.
    pub async fn update_project(
        &self,
        project_id: i32,
        body: serde_json::Value,
    ) -> Result<ProjectInfo, CliError> {
        let url = format!("{}/project/{}", self.base_url, project_id);

        let resp = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server PUT /project/{} failed ({}): {}",
                    project_id, status, body
                ),
            });
        }

        let api: ApiResponse<ProjectInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        api.item.ok_or_else(|| CliError::DeployFailed {
            target: crate::cli::config_parser::DeployTarget::Cloud,
            reason: "Stacker server updated project but returned no item".to_string(),
        })
    }

    // ── Cloud credentials ────────────────────────────

    /// List all saved cloud credentials for the authenticated user.
    pub async fn list_clouds(&self) -> Result<Vec<CloudInfo>, CliError> {
        let url = format!("{}/cloud", self.base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server GET /cloud failed ({}): {}", status, body),
            });
        }

        let api: ApiResponse<CloudInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        Ok(api.list.unwrap_or_default())
    }

    /// Find saved cloud credentials by provider name (e.g. "hetzner", "digital_ocean").
    pub async fn find_cloud_by_provider(
        &self,
        provider: &str,
    ) -> Result<Option<CloudInfo>, CliError> {
        let clouds = self.list_clouds().await?;
        let lower = provider.to_lowercase();
        Ok(clouds.into_iter().find(|c| c.provider.to_lowercase() == lower))
    }

    /// Find saved cloud credentials by ID.
    pub async fn get_cloud(&self, cloud_id: i32) -> Result<Option<CloudInfo>, CliError> {
        let url = format!("{}/cloud/{}", self.base_url, cloud_id);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server GET /cloud/{} failed ({}): {}",
                    cloud_id, status, body
                ),
            });
        }

        let api: ApiResponse<CloudInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        Ok(api.item)
    }

    /// Save cloud credentials to the Stacker server.
    pub async fn save_cloud(
        &self,
        provider: &str,
        cloud_token: Option<&str>,
        cloud_key: Option<&str>,
        cloud_secret: Option<&str>,
    ) -> Result<CloudInfo, CliError> {
        let url = format!("{}/cloud", self.base_url);

        let mut payload = serde_json::json!({
            "provider": provider,
            "save_token": true,
        });

        if let Some(obj) = payload.as_object_mut() {
            if let Some(t) = cloud_token {
                obj.insert(
                    "cloud_token".to_string(),
                    serde_json::Value::String(t.to_string()),
                );
            }
            if let Some(k) = cloud_key {
                obj.insert(
                    "cloud_key".to_string(),
                    serde_json::Value::String(k.to_string()),
                );
            }
            if let Some(s) = cloud_secret {
                obj.insert(
                    "cloud_secret".to_string(),
                    serde_json::Value::String(s.to_string()),
                );
            }
        }

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server POST /cloud failed ({}): {}",
                    status, body
                ),
            });
        }

        let api: ApiResponse<CloudInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        api.item.ok_or_else(|| CliError::DeployFailed {
            target: crate::cli::config_parser::DeployTarget::Cloud,
            reason: "Stacker server saved cloud but returned no item".to_string(),
        })
    }

    // ── Servers ──────────────────────────────────────

    /// List all servers for the authenticated user.
    pub async fn list_servers(&self) -> Result<Vec<ServerInfo>, CliError> {
        let url = format!("{}/server", self.base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server GET /server failed ({}): {}", status, body),
            });
        }

        let api: ApiResponse<ServerInfo> = resp.json().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            }
        })?;

        Ok(api.list.unwrap_or_default())
    }

    /// Find a server by name (case-insensitive).
    pub async fn find_server_by_name(&self, name: &str) -> Result<Option<ServerInfo>, CliError> {
        let servers = self.list_servers().await?;
        let lower = name.to_lowercase();
        Ok(servers.into_iter().find(|s| {
            s.name
                .as_deref()
                .map(|n| n.to_lowercase() == lower)
                .unwrap_or(false)
        }))
    }

    // ── Deploy ───────────────────────────────────────

    /// Deploy a project. If `cloud_id` is provided, uses saved cloud credentials.
    pub async fn deploy(
        &self,
        project_id: i32,
        cloud_id: Option<i32>,
        deploy_form: serde_json::Value,
    ) -> Result<DeployResponse, CliError> {
        let url = match cloud_id {
            Some(cid) => format!("{}/project/{}/deploy/{}", self.base_url, project_id, cid),
            None => format!("{}/project/{}/deploy", self.base_url, project_id),
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&deploy_form)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server deploy failed ({}): {}",
                    status, body
                ),
            });
        }

        resp.json::<DeployResponse>().await.map_err(|e| {
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid deploy response from Stacker server: {}", e),
            }
        })
    }

    // ── Deployment status ────────────────────────────

    /// Fetch deployment status by deployment ID.
    /// Returns `GET /api/v1/deployments/{id}`.
    pub async fn get_deployment_status(
        &self,
        deployment_id: i32,
    ) -> Result<Option<DeploymentStatusInfo>, CliError> {
        let url = format!("{}/api/v1/deployments/{}", self.base_url, deployment_id);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server GET /api/v1/deployments/{} failed ({}): {}",
                    deployment_id, status, body
                ),
            });
        }

        let api: ApiResponse<DeploymentStatusInfo> =
            resp.json().await.map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            })?;

        Ok(api.item)
    }

    /// Fetch the latest deployment status for a project.
    /// Returns `GET /api/v1/deployments/project/{project_id}`.
    pub async fn get_deployment_status_by_project(
        &self,
        project_id: i32,
    ) -> Result<Option<DeploymentStatusInfo>, CliError> {
        let url = format!(
            "{}/api/v1/deployments/project/{}",
            self.base_url, project_id
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Stacker server unreachable: {}", e),
            })?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!(
                    "Stacker server GET /api/v1/deployments/project/{} failed ({}): {}",
                    project_id, status, body
                ),
            });
        }

        let api: ApiResponse<DeploymentStatusInfo> =
            resp.json().await.map_err(|e| CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: format!("Invalid response from Stacker server: {}", e),
            })?;

        Ok(api.item)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Helper: build deploy form from stacker.yml config
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use crate::cli::config_parser::{ServiceDefinition, StackerConfig};

/// Generate a short unique ID for app entries (similar to Stacker UI IDs).
fn generate_app_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("cli_{:x}", ts)
}

/// Parse a Docker image string like `user/repo:tag`, `repo:tag`, or `repo`
/// into (dockerhub_user, dockerhub_name) tuple.
fn parse_docker_image(image: &str) -> (Option<String>, String) {
    // Handle images like "user/repo:tag" or "repo:tag" or "repo"
    if let Some((user_part, repo_part)) = image.split_once('/') {
        // Could be "namespace/repo:tag" or "registry.io/repo:tag"
        // If it looks like a registry (contains dots), treat the whole thing as image
        if user_part.contains('.') {
            (None, image.to_string())
        } else {
            (Some(user_part.to_string()), repo_part.to_string())
        }
    } else {
        (None, image.to_string())
    }
}

/// Parse a port mapping string like "8080:80", "8080:80/tcp", or "3000"
/// into (host_port, container_port) tuple.
fn parse_port_mapping(port_str: &str) -> (String, String) {
    // Remove protocol suffix like "/tcp", "/udp"
    let port_no_proto = port_str.split('/').next().unwrap_or(port_str);
    if let Some((host, container)) = port_no_proto.split_once(':') {
        (host.to_string(), container.to_string())
    } else {
        (port_no_proto.to_string(), port_no_proto.to_string())
    }
}

/// Parse a volume mapping string like "./dist:/usr/share/nginx/html" or "data:/var/lib/db"
/// into (host_path, container_path) tuple.
fn parse_volume_mapping(vol_str: &str) -> (String, String) {
    if let Some((host, container)) = vol_str.split_once(':') {
        (host.to_string(), container.to_string())
    } else {
        (vol_str.to_string(), vol_str.to_string())
    }
}

/// Convert a `ServiceDefinition` from stacker.yml into the Stacker server's
/// app JSON format (matching `forms::project::App` / `forms::project::Web`).
fn service_to_app_json(svc: &ServiceDefinition, network_ids: &[String]) -> serde_json::Value {
    let (dockerhub_user, dockerhub_name) = parse_docker_image(&svc.image);
    let id = generate_app_id();

    let shared_ports: Vec<serde_json::Value> = svc
        .ports
        .iter()
        .map(|p| {
            let (host, container) = parse_port_mapping(p);
            serde_json::json!({
                "host_port": host,
                "container_port": container,
            })
        })
        .collect();

    let volumes: Vec<serde_json::Value> = svc
        .volumes
        .iter()
        .map(|v| {
            let (host, container) = parse_volume_mapping(v);
            serde_json::json!({
                "host_path": host,
                "container_path": container,
            })
        })
        .collect();

    let environment: Vec<serde_json::Value> = svc
        .environment
        .iter()
        .map(|(k, v)| {
            serde_json::json!({
                "key": k,
                "value": v,
            })
        })
        .collect();

    let mut app = serde_json::json!({
        "_id": id,
        "name": svc.name.clone(),
        "code": svc.name.to_lowercase(),
        "type": "web",
        "dockerhub_name": dockerhub_name,
        "restart": "always",
        "custom": true,
        "shared_ports": shared_ports,
        "volumes": volumes,
        "environment": environment,
        "network": network_ids,
    });

    if let Some(user) = dockerhub_user {
        app.as_object_mut()
            .unwrap()
            .insert("dockerhub_user".to_string(), serde_json::json!(user));
    }

    app
}

/// Build the project creation body (matching `forms::project::ProjectForm`)
/// from the CLI's `StackerConfig`, including services from stacker.yml.
pub fn build_project_body(config: &StackerConfig) -> serde_json::Value {
    let stack_code = config
        .project
        .identity
        .clone()
        .unwrap_or_else(|| config.name.clone());

    // Create a default network
    let network_id = format!("cli_net_{:x}", {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    });

    let network_ids = vec![network_id.clone()];

    // Convert services from stacker.yml to Stacker server app format
    let web_apps: Vec<serde_json::Value> = config
        .services
        .iter()
        .map(|svc| service_to_app_json(svc, &network_ids))
        .collect();

    serde_json::json!({
        "custom": {
            "custom_stack_code": stack_code,
            "project_name": config.name.clone(),
            "web": web_apps,
            "feature": [],
            "service": [],
            "networks": [{
                "id": network_id,
                "name": "default_network",
                "driver": "bridge",
            }],
        }
    })
}

/// Build the deploy form payload that matches the Stacker server's
/// `forms::project::Deploy` structure.
pub fn build_deploy_form(config: &StackerConfig) -> serde_json::Value {
    let cloud = config.deploy.cloud.as_ref();
    let provider = cloud
        .map(|c| super::install_runner::provider_code_for_remote(&c.provider.to_string()).to_string())
        .unwrap_or_else(|| "htz".to_string());
    let region = cloud.and_then(|c| c.region.clone()).unwrap_or_else(|| "nbg1".to_string());
    let server_size = cloud.and_then(|c| c.size.clone()).unwrap_or_else(|| "cx22".to_string());
    let os = match provider.as_str() {
        "do" => "docker-20-04",
        _ => "ubuntu-22.04",
    };

    serde_json::json!({
        "cloud": {
            "provider": provider,
            "save_token": true,
        },
        "server": {
            "region": region,
            "server": server_size,
            "os": os,
        },
        "stack": {
            "stack_code": config.project.identity.clone().unwrap_or_else(|| config.name.clone()),
            "vars": [],
            "integrated_features": [],
            "extended_features": [],
            "subscriptions": [],
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_deploy_form_defaults() {
        let config = crate::cli::config_parser::ConfigBuilder::new()
            .name("myproject")
            .deploy_target(crate::cli::config_parser::DeployTarget::Cloud)
            .cloud(crate::cli::config_parser::CloudConfig {
                provider: crate::cli::config_parser::CloudProvider::Hetzner,
                orchestrator: crate::cli::config_parser::CloudOrchestrator::Remote,
                region: Some("fsn1".to_string()),
                size: Some("cx22".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
            })
            .build()
            .unwrap();

        let form = build_deploy_form(&config);
        assert_eq!(form["cloud"]["provider"], "htz");
        assert_eq!(form["server"]["region"], "fsn1");
        assert_eq!(form["server"]["server"], "cx22");
        assert_eq!(form["stack"]["stack_code"], "myproject");
    }

    #[test]
    fn test_build_deploy_form_with_identity() {
        let config = crate::cli::config_parser::ConfigBuilder::new()
            .name("myproject")
            .deploy_target(crate::cli::config_parser::DeployTarget::Cloud)
            .cloud(crate::cli::config_parser::CloudConfig {
                provider: crate::cli::config_parser::CloudProvider::Hetzner,
                orchestrator: crate::cli::config_parser::CloudOrchestrator::Remote,
                region: None,
                size: None,
                install_image: None,
                remote_payload_file: None,
                ssh_key: None,
                key: None,
                server: None,
            })
            .project_identity("optimumcode")
            .build()
            .unwrap();

        let form = build_deploy_form(&config);
        assert_eq!(form["stack"]["stack_code"], "optimumcode");
    }
}
