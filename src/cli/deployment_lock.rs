use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::cli::config_parser::{ServerConfig, StackerConfig, MARKETPLACE_ORIGIN_MARKER};
use crate::cli::error::CliError;
use crate::cli::install_runner::DeployResult;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeploymentLock — persisted deployment context
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Legacy filename for the deployment lockfile inside `.stacker/`.
pub const LOCKFILE_NAME: &str = "deployment.lock";

/// Returns the per-target lockfile name, e.g. `deployment-cloud.lock`.
pub fn lockfile_name_for_target(target: &str) -> String {
    format!("deployment-{}.lock", target)
}

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

    /// SSH private key path for the target server when explicitly configured.
    #[serde(default)]
    pub ssh_key: Option<PathBuf>,

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

    /// Stacker account email used for the deployment.
    pub stacker_email: Option<String>,

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
            ssh_key: None,
            server_name: None,
            deployment_id: result.deployment_id,
            project_id: result.project_id,
            cloud_id: None,
            project_name: None,
            stacker_email: None,
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
            ssh_key: None,
            server_name: None,
            deployment_id: None,
            project_id: None,
            cloud_id: None,
            project_name: None,
            stacker_email: None,
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
            ssh_key: server_cfg.ssh_key.clone(),
            server_name: None,
            deployment_id: None,
            project_id: None,
            cloud_id: None,
            project_name: None,
            stacker_email: None,
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

    pub fn with_stacker_email(mut self, email: Option<String>) -> Self {
        if email.is_some() {
            self.stacker_email = email;
        }
        self
    }

    // ── Persistence ──────────────────────────────────

    /// Resolve the per-target lockfile path (e.g. `.stacker/deployment-cloud.lock`).
    pub fn lockfile_path_for_target(project_dir: &Path, target: &str) -> PathBuf {
        project_dir
            .join(".stacker")
            .join(lockfile_name_for_target(target))
    }

    /// Legacy lockfile path (`.stacker/deployment.lock`).
    pub fn lockfile_path(project_dir: &Path) -> PathBuf {
        project_dir.join(".stacker").join(LOCKFILE_NAME)
    }

    /// Save the lock to `.stacker/deployment-{target}.lock`.
    pub fn save(&self, project_dir: &Path) -> Result<PathBuf, CliError> {
        let path = Self::lockfile_path_for_target(project_dir, &self.target);

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

    /// Load a deployment lock for a specific target.
    /// Falls back to the legacy `deployment.lock` if the per-target file doesn't exist.
    pub fn load_for_target(project_dir: &Path, target: &str) -> Result<Option<Self>, CliError> {
        let target_path = Self::lockfile_path_for_target(project_dir, target);
        if target_path.exists() {
            let content = std::fs::read_to_string(&target_path).map_err(CliError::Io)?;
            let lock: Self = serde_yaml::from_str(&content).map_err(|e| {
                CliError::ConfigValidation(format!(
                    "Failed to parse deployment lock ({}): {}. Delete the file and redeploy.",
                    target_path.display(),
                    e
                ))
            })?;
            return Ok(Some(lock));
        }

        // Fallback: try legacy deployment.lock (only if its target matches)
        Self::load_legacy(project_dir, Some(target))
    }

    /// Load the legacy `deployment.lock`, optionally filtering by target.
    fn load_legacy(
        project_dir: &Path,
        filter_target: Option<&str>,
    ) -> Result<Option<Self>, CliError> {
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

        if let Some(target) = filter_target {
            if lock.target != target {
                return Ok(None);
            }
        }

        Ok(Some(lock))
    }

    /// Load a deployment lock from `.stacker/deployment.lock` (legacy).
    /// Returns `None` if the file does not exist.
    pub fn load(project_dir: &Path) -> Result<Option<Self>, CliError> {
        // Try all per-target files first, then fall back to legacy
        for target in &["cloud", "server", "local"] {
            let target_path = Self::lockfile_path_for_target(project_dir, target);
            if target_path.exists() {
                let content = std::fs::read_to_string(&target_path).map_err(CliError::Io)?;
                let lock: Self = serde_yaml::from_str(&content).map_err(|e| {
                    CliError::ConfigValidation(format!(
                        "Failed to parse deployment lock ({}): {}. Delete the file and redeploy.",
                        target_path.display(),
                        e
                    ))
                })?;
                return Ok(Some(lock));
            }
        }

        Self::load_legacy(project_dir, None)
    }

    /// Load the lock for the active target if present, otherwise fall back to the
    /// first available lock.
    pub fn load_active(project_dir: &Path) -> Result<Option<Self>, CliError> {
        if let Some(target) = Self::read_active_target(project_dir)? {
            if let Some(lock) = Self::load_for_target(project_dir, &target)? {
                return Ok(Some(lock));
            }
        }

        Self::load(project_dir)
    }

    /// Check whether a lockfile exists for a given target.
    pub fn exists_for_target(project_dir: &Path, target: &str) -> bool {
        Self::lockfile_path_for_target(project_dir, target).exists()
    }

    /// Check whether any lockfile exists for this project (per-target or legacy).
    pub fn exists(project_dir: &Path) -> bool {
        for target in &["cloud", "server", "local"] {
            if Self::lockfile_path_for_target(project_dir, target).exists() {
                return true;
            }
        }
        Self::lockfile_path(project_dir).exists()
    }

    // ── Active Target ────────────────────────────────

    /// Path to the active-target file: `.stacker/active-target`
    pub fn active_target_path(project_dir: &Path) -> PathBuf {
        project_dir.join(".stacker").join("active-target")
    }

    /// Read the current active target (local, cloud, or server).
    /// Returns `None` if no active-target file exists.
    pub fn read_active_target(project_dir: &Path) -> Result<Option<String>, CliError> {
        let path = Self::active_target_path(project_dir);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path).map_err(CliError::Io)?;
        let target = content.trim().to_string();
        if target.is_empty() {
            Ok(None)
        } else {
            Ok(Some(target))
        }
    }

    /// Write the active target to `.stacker/active-target`.
    pub fn write_active_target(project_dir: &Path, target: &str) -> Result<(), CliError> {
        let path = Self::active_target_path(project_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(CliError::Io)?;
        }
        std::fs::write(&path, target).map_err(CliError::Io)?;
        Ok(())
    }

    /// Switch active target. For `local`, also creates the lock if missing.
    pub fn switch_target(project_dir: &Path, target: &str) -> Result<(), CliError> {
        match target {
            "local" => {
                if !Self::exists_for_target(project_dir, "local") {
                    let lock = Self::for_local();
                    lock.save(project_dir)?;
                }
            }
            "cloud" => {
                if !Self::exists_for_target(project_dir, target) {
                    return Err(CliError::ConfigValidation(format!(
                        "No {} deployment lock found. Deploy to {} first before switching.",
                        target, target
                    )));
                }
            }
            "server" => {
                if !Self::exists_for_target(project_dir, "server")
                    && Self::create_server_lock_from_default_config(project_dir)?.is_none()
                {
                    return Err(CliError::ConfigValidation(
                        "No server deployment lock found and stacker.yml does not define deploy.server. Configure the server first with `stacker config setup server`, add deploy.server to stacker.yml, or deploy to the server once before switching.".to_string(),
                    ));
                }
            }
            _ => {
                return Err(CliError::ConfigValidation(format!(
                    "Unknown target '{}'. Use: local, cloud, or server.",
                    target
                )));
            }
        }
        Self::write_active_target(project_dir, target)
    }

    /// Load server connection details from a server lock when they are complete
    /// enough to stand in for `deploy.server` in stacker.yml.
    pub fn to_server_config(&self) -> Option<ServerConfig> {
        if self.target != "server" {
            return None;
        }

        let host = self.server_ip.as_ref()?.trim();
        if host.is_empty() || host == "127.0.0.1" {
            return None;
        }

        Some(ServerConfig {
            host: host.to_string(),
            user: self
                .ssh_user
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "root".to_string()),
            ssh_key: self.ssh_key.clone(),
            port: self.ssh_port.unwrap_or(22),
        })
    }

    fn create_server_lock_from_default_config(
        project_dir: &Path,
    ) -> Result<Option<Self>, CliError> {
        let config_path = project_dir.join("stacker.yml");
        if !config_path.exists() {
            return Ok(None);
        }

        let config = StackerConfig::from_file(&config_path)?;
        let Some(server_cfg) = config.deploy.server.as_ref() else {
            return Ok(None);
        };

        let lock = Self::for_server(server_cfg);
        lock.save(project_dir)?;
        Ok(Some(lock))
    }

    // ── Config update ────────────────────────────────

    /// Apply a targeted, formatting-preserving edit to a stacker.yml file.
    ///
    /// Unlike [`write_config`], this parses the file into a `serde_yaml::Value`,
    /// lets `mutate` change only the keys it cares about, and writes the result
    /// back. Key order, scalar quoting, and — crucially — any keys not modeled
    /// by `StackerConfig` (e.g. `config_contract`) are preserved untouched. A
    /// `.yml.bak` backup is written before overwriting.
    ///
    /// This is the preferred path for in-place edits (`config setup server`,
    /// `config unlock`); `write_config` round-trips the whole typed struct and
    /// therefore reorders sections, drops unmodeled keys, and strips comments.
    pub fn edit_config_value(
        config_path: &Path,
        mutate: impl FnOnce(&mut serde_yaml::Mapping) -> Result<(), CliError>,
    ) -> Result<(), CliError> {
        let raw = std::fs::read_to_string(config_path).map_err(CliError::Io)?;
        // serde_yaml drops comments on round-trip. The `@stacker-origin`
        // marker is a comment but carries trust semantics (it gates hook
        // execution), so preserve it explicitly rather than silently
        // downgrading a marketplace-origin file to "user-authored".
        let had_origin_marker = raw.contains(MARKETPLACE_ORIGIN_MARKER);
        let mut doc: serde_yaml::Value = serde_yaml::from_str(&raw)
            .map_err(|e| CliError::ConfigValidation(format!("Failed to parse config: {}", e)))?;
        let root = doc.as_mapping_mut().ok_or_else(|| {
            CliError::ConfigValidation("stacker.yml root must be a mapping".to_string())
        })?;

        mutate(root)?;

        if config_path.exists() {
            let backup_path = config_path.with_extension("yml.bak");
            std::fs::copy(config_path, &backup_path).map_err(CliError::Io)?;
        }

        let mut yaml = serde_yaml::to_string(&doc).map_err(|e| {
            CliError::ConfigValidation(format!("Failed to serialize config: {}", e))
        })?;
        if had_origin_marker && !yaml.contains(MARKETPLACE_ORIGIN_MARKER) {
            yaml = format!("{}\n{}", MARKETPLACE_ORIGIN_MARKER, yaml);
        }
        std::fs::write(config_path, &yaml).map_err(CliError::Io)?;

        Ok(())
    }

    /// Set `deploy.target: server` and `deploy.server` in stacker.yml while
    /// leaving every other section byte-for-byte intact. Used by
    /// `config setup server`.
    pub fn set_deploy_server(
        config_path: &Path,
        server_cfg: &ServerConfig,
    ) -> Result<(), CliError> {
        Self::edit_config_value(config_path, |root| {
            let deploy = ensure_child_mapping(root, "deploy");
            deploy.insert(
                yaml_key("target"),
                serde_yaml::Value::String("server".to_string()),
            );

            let mut server_value = serde_yaml::to_value(server_cfg).map_err(|e| {
                CliError::ConfigValidation(format!("Failed to serialize server config: {}", e))
            })?;
            prune_value_noise(&mut server_value);
            deploy.insert(yaml_key("server"), server_value);
            Ok(())
        })
    }

    /// Remove the `deploy.server` section from stacker.yml, preserving all other
    /// content. Used by `config unlock`.
    pub fn remove_deploy_server(config_path: &Path) -> Result<(), CliError> {
        Self::edit_config_value(config_path, |root| {
            if let Some(deploy) = root
                .get_mut(&yaml_key("deploy"))
                .and_then(|value| value.as_mapping_mut())
            {
                deploy.remove(&yaml_key("server"));
            }
            Ok(())
        })
    }

    /// Persist this lock's server details into `deploy.server` in stacker.yml,
    /// preserving all unrelated content. Value-based equivalent of
    /// `apply_to_config` + `write_config`, used by `--lock` and `config
    /// apply-lock`.
    ///
    /// No-op (returns `Ok(false)`) for local deploys — when `server_ip` is
    /// missing or `127.0.0.1`. Reads the file raw, so `${VAR}` placeholders in
    /// unrelated fields are never resolved/baked into the written file.
    ///
    /// `ssh_key` precedence mirrors `apply_to_config`: an existing
    /// `deploy.server.ssh_key` wins, then this lock's `ssh_key`, then
    /// `deploy.cloud.ssh_key`.
    pub fn persist_server_to_config(&self, config_path: &Path) -> Result<bool, CliError> {
        let Some(ip) = self.server_ip.as_deref() else {
            return Ok(false);
        };
        if ip == "127.0.0.1" {
            return Ok(false);
        }

        let host = ip.to_string();
        let user = self.ssh_user.clone().unwrap_or_else(|| "root".to_string());
        let port = self.ssh_port.unwrap_or(22);
        let lock_ssh_key = self
            .ssh_key
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());

        Self::edit_config_value(config_path, |root| {
            let deploy = ensure_child_mapping(root, "deploy");

            let existing_server_key = child_string(deploy, "server", "ssh_key");
            let cloud_key = child_string(deploy, "cloud", "ssh_key");
            let ssh_key = existing_server_key.or(lock_ssh_key).or(cloud_key);

            let mut server = serde_yaml::Mapping::new();
            server.insert(yaml_key("host"), serde_yaml::Value::String(host));
            server.insert(yaml_key("user"), serde_yaml::Value::String(user));
            if let Some(key) = ssh_key {
                server.insert(yaml_key("ssh_key"), serde_yaml::Value::String(key));
            }
            server.insert(
                yaml_key("port"),
                serde_yaml::Value::Number(serde_yaml::Number::from(port)),
            );

            deploy.insert(yaml_key("server"), serde_yaml::Value::Mapping(server));
            Ok(())
        })?;

        Ok(true)
    }
}

/// Build a `serde_yaml::Value` string key.
fn yaml_key(key: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(key.to_string())
}

/// Read `parent[section][field]` as a string, if present.
fn child_string(parent: &serde_yaml::Mapping, section: &str, field: &str) -> Option<String> {
    parent
        .get(&yaml_key(section))
        .and_then(|value| value.as_mapping())
        .and_then(|map| map.get(&yaml_key(field)))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

/// Ensure `parent[key]` exists and is a mapping, returning a mutable reference
/// to it. If the key is absent it is appended (preserving existing key order);
/// if present but not a mapping it is replaced with an empty mapping.
fn ensure_child_mapping<'a>(
    parent: &'a mut serde_yaml::Mapping,
    key: &str,
) -> &'a mut serde_yaml::Mapping {
    let k = yaml_key(key);
    let is_mapping = parent.get(&k).map(|v| v.is_mapping()).unwrap_or(false);
    if !is_mapping {
        parent.insert(
            k.clone(),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }
    parent
        .get_mut(&k)
        .and_then(|value| value.as_mapping_mut())
        .expect("child mapping ensured above")
}

/// Drop null values and empty collections from the top level of a mapping so a
/// freshly serialized section (e.g. `deploy.server`) doesn't carry `ssh_key:
/// null` and similar noise into the file.
fn prune_value_noise(value: &mut serde_yaml::Value) {
    let serde_yaml::Value::Mapping(map) = value else {
        return;
    };
    let keys: Vec<serde_yaml::Value> = map.keys().cloned().collect();
    for key in keys {
        let is_noise = match map.get(&key) {
            Some(serde_yaml::Value::Null) => true,
            Some(serde_yaml::Value::Sequence(items)) => items.is_empty(),
            Some(serde_yaml::Value::Mapping(inner)) => inner.is_empty(),
            _ => false,
        };
        if is_noise {
            map.remove(&key);
        }
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
            ssh_key: None,
            server_name: Some("my-server".to_string()),
            deployment_id: Some(123),
            project_id: Some(456),
            cloud_id: Some(7),
            project_name: Some("my-project".to_string()),
            stacker_email: Some("owner@example.com".to_string()),
            deployed_at: "2026-03-06T12:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let tmp = TempDir::new().unwrap();
        let lock = sample_lock();

        let path = lock.save(tmp.path()).unwrap();
        assert!(path.exists());
        assert!(path.ends_with("deployment-cloud.lock"));

        let loaded = DeploymentLock::load_for_target(tmp.path(), "cloud")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.server_ip, lock.server_ip);
        assert_eq!(loaded.deployment_id, lock.deployment_id);
        assert_eq!(loaded.project_id, lock.project_id);
        assert_eq!(loaded.server_name, lock.server_name);
        assert_eq!(loaded.stacker_email, lock.stacker_email);
        assert_eq!(loaded.target, "cloud");
    }

    #[test]
    fn load_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let result = DeploymentLock::load(tmp.path()).unwrap();
        assert!(result.is_none());
        let result = DeploymentLock::load_for_target(tmp.path(), "cloud").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn exists_detection() {
        let tmp = TempDir::new().unwrap();
        assert!(!DeploymentLock::exists(tmp.path()));
        assert!(!DeploymentLock::exists_for_target(tmp.path(), "cloud"));

        sample_lock().save(tmp.path()).unwrap();
        assert!(DeploymentLock::exists(tmp.path()));
        assert!(DeploymentLock::exists_for_target(tmp.path(), "cloud"));
        assert!(!DeploymentLock::exists_for_target(tmp.path(), "local"));
    }

    #[test]
    fn local_and_cloud_locks_coexist() {
        let tmp = TempDir::new().unwrap();

        // Save cloud lock
        let cloud_lock = sample_lock();
        cloud_lock.save(tmp.path()).unwrap();

        // Save local lock
        let local_lock = DeploymentLock::for_local();
        local_lock.save(tmp.path()).unwrap();

        // Both exist
        assert!(DeploymentLock::exists_for_target(tmp.path(), "cloud"));
        assert!(DeploymentLock::exists_for_target(tmp.path(), "local"));

        // Load each independently
        let loaded_cloud = DeploymentLock::load_for_target(tmp.path(), "cloud")
            .unwrap()
            .unwrap();
        assert_eq!(loaded_cloud.server_ip, Some("203.0.113.42".to_string()));
        assert_eq!(loaded_cloud.deployment_id, Some(123));

        let loaded_local = DeploymentLock::load_for_target(tmp.path(), "local")
            .unwrap()
            .unwrap();
        assert_eq!(loaded_local.server_ip, Some("127.0.0.1".to_string()));
        assert_eq!(loaded_local.deployment_id, None);

        // Generic load() prefers cloud over local
        let generic = DeploymentLock::load(tmp.path()).unwrap().unwrap();
        assert_eq!(generic.target, "cloud");
    }

    #[test]
    fn legacy_lockfile_fallback() {
        let tmp = TempDir::new().unwrap();

        // Manually write a legacy deployment.lock
        let stacker_dir = tmp.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        let legacy_lock = sample_lock();
        let content = serde_yaml::to_string(&legacy_lock).unwrap();
        std::fs::write(stacker_dir.join("deployment.lock"), &content).unwrap();

        // load_for_target("cloud") should find it via legacy fallback
        let loaded = DeploymentLock::load_for_target(tmp.path(), "cloud")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.target, "cloud");
        assert_eq!(loaded.deployment_id, Some(123));

        // load_for_target("local") should NOT find it (target mismatch)
        let loaded_local = DeploymentLock::load_for_target(tmp.path(), "local").unwrap();
        assert!(loaded_local.is_none());

        // Generic load() should find the legacy file
        let generic = DeploymentLock::load(tmp.path()).unwrap().unwrap();
        assert_eq!(generic.target, "cloud");
    }

    #[test]
    fn persist_server_to_config_writes_server_section() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        std::fs::write(&path, "name: ghost\ndeploy:\n  target: cloud\n").unwrap();

        let wrote = sample_lock().persist_server_to_config(&path).unwrap();
        assert!(wrote);

        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &doc["deploy"]["server"];
        assert_eq!(server["host"], "203.0.113.42");
        assert_eq!(server["user"], "root");
        assert_eq!(server["port"], 22);
        assert!(server.get("ssh_key").is_none());
    }

    #[test]
    fn persist_server_to_config_ssh_key_precedence_prefers_existing_server_key() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        // Both an existing deploy.server.ssh_key and deploy.cloud.ssh_key exist;
        // the existing server key must win over the lock's and the cloud's.
        std::fs::write(
            &path,
            "deploy:\n  cloud:\n    ssh_key: ~/.ssh/cloud\n  server:\n    host: old\n    ssh_key: ~/.ssh/existing\n",
        )
        .unwrap();

        let mut lock = sample_lock();
        lock.ssh_key = Some(PathBuf::from("~/.ssh/from_lock"));
        lock.persist_server_to_config(&path).unwrap();

        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["deploy"]["server"]["ssh_key"], "~/.ssh/existing");
        // Unrelated deploy.cloud section is preserved.
        assert_eq!(doc["deploy"]["cloud"]["ssh_key"], "~/.ssh/cloud");
    }

    #[test]
    fn persist_server_to_config_skips_local() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        std::fs::write(&path, "name: ghost\ndeploy:\n  target: local\n").unwrap();

        let wrote = DeploymentLock::for_local()
            .persist_server_to_config(&path)
            .unwrap();
        assert!(!wrote);

        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(doc["deploy"].get("server").is_none());
    }

    #[test]
    fn for_server_captures_config() {
        let server_cfg = ServerConfig {
            host: "10.0.0.1".to_string(),
            user: "deploy".to_string(),
            ssh_key: Some(PathBuf::from("/tmp/id_ed25519")),
            port: 2222,
        };

        let lock = DeploymentLock::for_server(&server_cfg);
        assert_eq!(lock.server_ip, Some("10.0.0.1".to_string()));
        assert_eq!(lock.ssh_user, Some("deploy".to_string()));
        assert_eq!(lock.ssh_port, Some(2222));
        assert_eq!(lock.ssh_key, Some(PathBuf::from("/tmp/id_ed25519")));
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

    #[test]
    fn with_stacker_email_enriches_lock() {
        let lock = DeploymentLock::for_local().with_stacker_email(Some("user@example.com".into()));
        assert_eq!(lock.stacker_email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn active_target_read_write() {
        let tmp = TempDir::new().unwrap();

        // No active target initially
        assert_eq!(
            DeploymentLock::read_active_target(tmp.path()).unwrap(),
            None
        );

        // Write and read back
        DeploymentLock::write_active_target(tmp.path(), "local").unwrap();
        assert_eq!(
            DeploymentLock::read_active_target(tmp.path()).unwrap(),
            Some("local".to_string())
        );

        // Switch to cloud
        DeploymentLock::write_active_target(tmp.path(), "cloud").unwrap();
        assert_eq!(
            DeploymentLock::read_active_target(tmp.path()).unwrap(),
            Some("cloud".to_string())
        );
    }

    #[test]
    fn switch_target_creates_local_lock() {
        let tmp = TempDir::new().unwrap();

        // Switch to local — should create the lock automatically
        DeploymentLock::switch_target(tmp.path(), "local").unwrap();
        assert!(DeploymentLock::exists_for_target(tmp.path(), "local"));
        assert_eq!(
            DeploymentLock::read_active_target(tmp.path()).unwrap(),
            Some("local".to_string())
        );
    }

    #[test]
    fn switch_target_cloud_requires_existing_lock() {
        let tmp = TempDir::new().unwrap();

        // Switch to cloud without a lock should fail
        let result = DeploymentLock::switch_target(tmp.path(), "cloud");
        assert!(result.is_err());
    }

    #[test]
    fn switch_target_server_creates_lock_from_config() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("stacker.yml"),
            r#"
name: demo
app:
  type: custom
  image: ghcr.io/example/demo:latest
deploy:
  target: local
  server:
    host: 192.0.2.10
    user: deploy
    port: 2222
    ssh_key: ~/.ssh/demo
"#,
        )
        .unwrap();

        DeploymentLock::switch_target(tmp.path(), "server").unwrap();

        let lock = DeploymentLock::load_for_target(tmp.path(), "server")
            .unwrap()
            .unwrap();
        assert_eq!(lock.server_ip.as_deref(), Some("192.0.2.10"));
        assert_eq!(lock.ssh_user.as_deref(), Some("deploy"));
        assert_eq!(lock.ssh_port, Some(2222));
        assert_eq!(lock.ssh_key, Some(PathBuf::from("~/.ssh/demo")));
        assert_eq!(
            DeploymentLock::read_active_target(tmp.path()).unwrap(),
            Some("server".to_string())
        );
    }

    #[test]
    fn switch_target_unknown_target_fails() {
        let tmp = TempDir::new().unwrap();
        let result = DeploymentLock::switch_target(tmp.path(), "mars");
        assert!(result.is_err());
    }

    #[test]
    fn load_active_prefers_active_target_lock() {
        let tmp = TempDir::new().unwrap();

        sample_lock().save(tmp.path()).unwrap();
        DeploymentLock::for_local().save(tmp.path()).unwrap();
        DeploymentLock::write_active_target(tmp.path(), "local").unwrap();

        let lock = DeploymentLock::load_active(tmp.path()).unwrap().unwrap();
        assert_eq!(lock.target, "local");
    }

    const RICH_CONFIG: &str = r#"# my project config
name: ghost
version: "1.0.0"
app:
  type: custom
  ports:
    - "8080:8080"
config_contract:
  services:
    mysql:
      required:
        - MYSQL_ROOT_PASSWORD
        - MYSQL_DATABASE
      secret:
        - MYSQL_ROOT_PASSWORD
deploy:
  target: cloud
  cloud:
    provider: hetzner
    orchestrator: remote
env:
  PAT: "${PAT}"
"#;

    #[test]
    fn set_deploy_server_preserves_unrelated_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        std::fs::write(&path, RICH_CONFIG).unwrap();

        let server_cfg = ServerConfig {
            host: "203.0.113.10".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        };
        DeploymentLock::set_deploy_server(&path, &server_cfg).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();

        // Only deploy.target/server changed.
        assert!(written.contains("target: server"));
        assert!(written.contains("host: 203.0.113.10"));
        // No null noise from the optional ssh_key field.
        assert!(!written.contains("ssh_key: null"));

        // Reparse and assert unrelated sections survive verbatim (this is the
        // regression: previously config_contract was gutted to `mysql: {}`).
        let doc: serde_yaml::Value = serde_yaml::from_str(&written).unwrap();
        let mysql = &doc["config_contract"]["services"]["mysql"];
        assert_eq!(mysql["required"][0], "MYSQL_ROOT_PASSWORD");
        assert_eq!(mysql["required"][1], "MYSQL_DATABASE");
        assert_eq!(mysql["secret"][0], "MYSQL_ROOT_PASSWORD");
        // Untouched deploy.cloud stays intact (no orchestrator injection, no drop).
        assert_eq!(doc["deploy"]["cloud"]["provider"], "hetzner");
        assert_eq!(doc["deploy"]["cloud"]["orchestrator"], "remote");

        // Untouched scalar values keep their type (serde_yaml may normalize the
        // quote *style*, but the string value must not be coerced to a number).
        assert_eq!(doc["version"].as_str(), Some("1.0.0"));
        assert_eq!(doc["app"]["ports"][0].as_str(), Some("8080:8080"));
        assert_eq!(doc["env"]["PAT"].as_str(), Some("${PAT}"));

        // A .bak backup was written.
        assert!(tmp.path().join("stacker.yml.bak").exists());
    }

    #[test]
    fn remove_deploy_server_drops_only_server_section() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        std::fs::write(&path, RICH_CONFIG).unwrap();

        let server_cfg = ServerConfig {
            host: "203.0.113.10".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        };
        DeploymentLock::set_deploy_server(&path, &server_cfg).unwrap();
        DeploymentLock::remove_deploy_server(&path).unwrap();

        let doc: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(doc["deploy"].get("server").is_none());
        // Sibling deploy keys and unrelated sections remain.
        assert_eq!(doc["deploy"]["cloud"]["provider"], "hetzner");
        assert_eq!(
            doc["config_contract"]["services"]["mysql"]["required"][0],
            "MYSQL_ROOT_PASSWORD"
        );
    }

    #[test]
    fn edit_config_value_preserves_origin_marker() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("stacker.yml");
        std::fs::write(
            &path,
            format!("{}\n{}", MARKETPLACE_ORIGIN_MARKER, RICH_CONFIG),
        )
        .unwrap();

        let server_cfg = ServerConfig {
            host: "203.0.113.10".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        };
        DeploymentLock::set_deploy_server(&path, &server_cfg).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        // The trust marker survives the edit (otherwise hooks would silently
        // become untrusted-but-runnable on the next deploy).
        assert!(written.contains(MARKETPLACE_ORIGIN_MARKER));
    }
}
