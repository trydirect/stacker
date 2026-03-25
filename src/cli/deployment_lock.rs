use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::cli::config_parser::{ServerConfig, StackerConfig};
use crate::cli::error::CliError;
use crate::cli::install_runner::DeployResult;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeploymentLock — persisted deployment context
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Filename for the deployment lockfile inside `.stacker/`.
pub const LOCKFILE_NAME: &str = "deployment.lock";

/// Persisted deployment context written after a successful deploy.
///
/// Lives in `.stacker/deployment.lock` and allows subsequent deploys
/// to reuse the same server without requiring manual stacker.yml edits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentLock {
    /// Deploy target that was used (local / cloud / server).
    pub target: String,

    /// IP address of the provisioned/used server.
    pub server_ip: Option<String>,

    /// SSH user on the target server.
    pub ssh_user: Option<String>,

    /// SSH port on the target server.
    pub ssh_port: Option<u16>,

    /// Server name on the Stacker platform (for `--server` reuse).
    pub server_name: Option<String>,

    /// Stacker server deployment ID.
    pub deployment_id: Option<i64>,

    /// Stacker server project ID.
    pub project_id: Option<i64>,

    /// Cloud credential ID used for this deployment.
    pub cloud_id: Option<i32>,

    /// Project name as known by the Stacker server.
    pub project_name: Option<String>,

    /// ISO 8601 timestamp of the deployment.
    pub deployed_at: String,
}

impl DeploymentLock {
    // ── Constructors ─────────────────────────────────

    /// Build a lock from a `DeployResult` (basic info available immediately after deploy).
    pub fn from_result(result: &DeployResult) -> Self {
        Self {
            target: format!("{:?}", result.target).to_lowercase(),
            server_ip: result.server_ip.clone(),
            ssh_user: None,
            ssh_port: None,
            server_name: None,
            deployment_id: result.deployment_id,
            project_id: result.project_id,
            cloud_id: None,
            project_name: None,
            deployed_at: Utc::now().to_rfc3339(),
        }
    }

    /// Build a lock for a local deploy.
    pub fn for_local() -> Self {
        Self {
            target: "local".to_string(),
            server_ip: Some("127.0.0.1".to_string()),
            ssh_user: None,
            ssh_port: None,
            server_name: None,
            deployment_id: None,
            project_id: None,
            cloud_id: None,
            project_name: None,
            deployed_at: Utc::now().to_rfc3339(),
        }
    }

    /// Build a lock for a server (SSH) deploy from the config.
    pub fn for_server(server_cfg: &ServerConfig) -> Self {
        Self {
            target: "server".to_string(),
            server_ip: Some(server_cfg.host.clone()),
            ssh_user: Some(server_cfg.user.clone()),
            ssh_port: Some(server_cfg.port),
            server_name: None,
            deployment_id: None,
            project_id: None,
            cloud_id: None,
            project_name: None,
            deployed_at: Utc::now().to_rfc3339(),
        }
    }

    // ── Enrichment (builder pattern) ─────────────────

    /// Enrich with server details fetched from the Stacker API.
    pub fn with_server_info(
        mut self,
        ip: Option<String>,
        user: Option<String>,
        port: Option<u16>,
        name: Option<String>,
        cloud_id: Option<i32>,
    ) -> Self {
        if ip.is_some() {
            self.server_ip = ip;
        }
        if user.is_some() {
            self.ssh_user = user;
        }
        if port.is_some() {
            self.ssh_port = port;
        }
        if name.is_some() {
            self.server_name = name;
        }
        if cloud_id.is_some() {
            self.cloud_id = cloud_id;
        }
        self
    }

    pub fn with_project_name(mut self, name: Option<String>) -> Self {
        if name.is_some() {
            self.project_name = name;
        }
        self
    }

    // ── Persistence ──────────────────────────────────

    /// Resolve the lockfile path inside `.stacker/` relative to the project dir.
    pub fn lockfile_path(project_dir: &Path) -> PathBuf {
        project_dir.join(".stacker").join(LOCKFILE_NAME)
    }

    /// Save the lock to `.stacker/deployment.lock`.
    pub fn save(&self, project_dir: &Path) -> Result<PathBuf, CliError> {
        let path = Self::lockfile_path(project_dir);

        // Ensure .stacker/ exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(CliError::Io)?;
        }

        let content = serde_yaml::to_string(self).map_err(|e| {
            CliError::ConfigValidation(format!("Failed to serialize deployment lock: {}", e))
        })?;

        std::fs::write(&path, &content).map_err(CliError::Io)?;

        Ok(path)
    }

    /// Load a deployment lock from `.stacker/deployment.lock`.
    /// Returns `None` if the file does not exist.
    pub fn load(project_dir: &Path) -> Result<Option<Self>, CliError> {
        let path = Self::lockfile_path(project_dir);

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path).map_err(CliError::Io)?;

        let lock: Self = serde_yaml::from_str(&content).map_err(|e| {
            CliError::ConfigValidation(format!(
                "Failed to parse deployment lock ({}): {}. Delete the file and redeploy.",
                path.display(),
                e
            ))
        })?;

        Ok(Some(lock))
    }

    /// Check whether a lockfile exists for this project.
    pub fn exists(project_dir: &Path) -> bool {
        Self::lockfile_path(project_dir).exists()
    }

    // ── Config update ────────────────────────────────

    /// Update a StackerConfig's `deploy.server` section from this lock.
    ///
    /// Used by `--lock` flag and `stacker config lock` to persist
    /// server details into stacker.yml for future SSH-based deploys.
    pub fn apply_to_config(&self, config: &mut StackerConfig) {
        if let Some(ref ip) = self.server_ip {
            if ip == "127.0.0.1" {
                // Local deploy — nothing to persist in server section
                return;
            }

            let ssh_key = config
                .deploy
                .server
                .as_ref()
                .and_then(|s| s.ssh_key.clone())
                .or_else(|| {
                    config
                        .deploy
                        .cloud
                        .as_ref()
                        .and_then(|c| c.ssh_key.clone())
                });

            config.deploy.server = Some(ServerConfig {
                host: ip.clone(),
                user: self.ssh_user.clone().unwrap_or_else(|| "root".to_string()),
                ssh_key,
                port: self.ssh_port.unwrap_or(22),
            });
        }
    }

    /// Write a StackerConfig back to disk (used after `apply_to_config`).
    ///
    /// Creates a `.bak` backup before overwriting.
    pub fn write_config(config: &StackerConfig, config_path: &Path) -> Result<(), CliError> {
        // Backup existing file
        if config_path.exists() {
            let backup_path = config_path.with_extension("yml.bak");
            std::fs::copy(config_path, &backup_path).map_err(CliError::Io)?;
        }

        let yaml = serde_yaml::to_string(config).map_err(|e| {
            CliError::ConfigValidation(format!("Failed to serialize config: {}", e))
        })?;

        std::fs::write(config_path, &yaml).map_err(CliError::Io)?;

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config_parser::DeployTarget;
    use tempfile::TempDir;

    fn sample_lock() -> DeploymentLock {
        DeploymentLock {
            target: "cloud".to_string(),
            server_ip: Some("203.0.113.42".to_string()),
            ssh_user: Some("root".to_string()),
            ssh_port: Some(22),
            server_name: Some("my-server".to_string()),
            deployment_id: Some(123),
            project_id: Some(456),
            cloud_id: Some(7),
            project_name: Some("my-project".to_string()),
            deployed_at: "2026-03-06T12:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let tmp = TempDir::new().unwrap();
        let lock = sample_lock();

        let path = lock.save(tmp.path()).unwrap();
        assert!(path.exists());

        let loaded = DeploymentLock::load(tmp.path()).unwrap().unwrap();
        assert_eq!(loaded.server_ip, lock.server_ip);
        assert_eq!(loaded.deployment_id, lock.deployment_id);
        assert_eq!(loaded.project_id, lock.project_id);
        assert_eq!(loaded.server_name, lock.server_name);
        assert_eq!(loaded.target, "cloud");
    }

    #[test]
    fn load_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let result = DeploymentLock::load(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn exists_detection() {
        let tmp = TempDir::new().unwrap();
        assert!(!DeploymentLock::exists(tmp.path()));

        sample_lock().save(tmp.path()).unwrap();
        assert!(DeploymentLock::exists(tmp.path()));
    }

    #[test]
    fn apply_to_config_sets_server_section() {
        let lock = sample_lock();
        let mut config = StackerConfig::default();

        lock.apply_to_config(&mut config);

        let server = config.deploy.server.unwrap();
        assert_eq!(server.host, "203.0.113.42");
        assert_eq!(server.user, "root");
        assert_eq!(server.port, 22);
    }

    #[test]
    fn apply_to_config_skips_local() {
        let lock = DeploymentLock::for_local();
        let mut config = StackerConfig::default();

        lock.apply_to_config(&mut config);

        assert!(config.deploy.server.is_none());
    }

    #[test]
    fn for_server_captures_config() {
        let server_cfg = ServerConfig {
            host: "10.0.0.1".to_string(),
            user: "deploy".to_string(),
            ssh_key: None,
            port: 2222,
        };

        let lock = DeploymentLock::for_server(&server_cfg);
        assert_eq!(lock.server_ip, Some("10.0.0.1".to_string()));
        assert_eq!(lock.ssh_user, Some("deploy".to_string()));
        assert_eq!(lock.ssh_port, Some(2222));
        assert_eq!(lock.target, "server");
    }

    #[test]
    fn with_server_info_enriches_lock() {
        let lock = DeploymentLock::from_result(&DeployResult {
            target: DeployTarget::Cloud,
            message: "deployed".to_string(),
            server_ip: None,
            deployment_id: Some(1),
            project_id: Some(2),
            server_name: None,
        });

        let enriched = lock.with_server_info(
            Some("1.2.3.4".to_string()),
            Some("ubuntu".to_string()),
            Some(22),
            Some("prod-01".to_string()),
            Some(99),
        );

        assert_eq!(enriched.server_ip, Some("1.2.3.4".to_string()));
        assert_eq!(enriched.ssh_user, Some("ubuntu".to_string()));
        assert_eq!(enriched.server_name, Some("prod-01".to_string()));
        assert_eq!(enriched.cloud_id, Some(99));
    }
}
