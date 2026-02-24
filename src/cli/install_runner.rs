use std::path::{Path, PathBuf};

use crate::cli::config_parser::{CloudOrchestrator, DeployTarget, StackerConfig};
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::stacker_client::{self, StackerClient};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Constants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Default Docker image for the install container (Terraform + Ansible).
pub const DEFAULT_INSTALL_IMAGE: &str = "trydirect/install-service:latest";

/// Mount point for stacker.yml inside the install container.
pub const CONTAINER_CONFIG_PATH: &str = "/app/stacker.yml";

/// Mount point for the compose file inside the install container.
pub const CONTAINER_COMPOSE_PATH: &str = "/app/docker-compose.yml";

/// Mount point for SSH keys inside the install container.
pub const CONTAINER_SSH_KEY_PATH: &str = "/root/.ssh/id_rsa";

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CommandExecutor — abstraction for running shell commands (DIP)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Result of executing a command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Abstraction over shell command execution.
///
/// Production: `ShellExecutor` runs commands via `std::process::Command`.
/// Tests: `MockExecutor` records commands for assertion without side effects.
pub trait CommandExecutor: Send + Sync {
    fn execute(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CliError>;
}

/// Production executor — actually runs docker commands.
pub struct ShellExecutor;

impl CommandExecutor for ShellExecutor {
    fn execute(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CliError> {
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| CliError::CommandFailed {
                command: format!("{} {} — {}", program, args.join(" "), e),
                exit_code: -1,
            })?;

        Ok(CommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeployContext — everything needed for a deployment
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Aggregated deployment context passed to strategies.
#[derive(Debug, Clone)]
pub struct DeployContext {
    /// Path to the stacker.yml config file.
    pub config_path: PathBuf,

    /// Path to the generated docker-compose.yml.
    pub compose_path: PathBuf,

    /// Working directory of the project.
    pub project_dir: PathBuf,

    /// Whether this is a dry-run (plan) or real deployment (apply).
    pub dry_run: bool,

    /// Install container image override.
    pub image: Option<String>,

    /// Remote deploy overrides from CLI flags.
    pub project_name_override: Option<String>,
    pub key_name_override: Option<String>,
    pub server_name_override: Option<String>,
}

impl DeployContext {
    pub fn install_image(&self) -> &str {
        self.image.as_deref().unwrap_or(DEFAULT_INSTALL_IMAGE)
    }
}

/// Outcome of a successful deployment.
#[derive(Debug, Clone)]
pub struct DeployResult {
    pub target: DeployTarget,
    pub message: String,
    pub server_ip: Option<String>,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeployStrategy — strategy pattern for deployment targets (OCP + DIP)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Each deployment target implements this trait.
/// New targets can be added without modifying existing code (OCP).
pub trait DeployStrategy {
    fn validate(&self, config: &StackerConfig) -> Result<(), CliError>;
    fn deploy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<DeployResult, CliError>;
    fn destroy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<(), CliError>;
}

/// Factory: map `DeployTarget` to its strategy implementation.
pub fn strategy_for(target: &DeployTarget) -> Box<dyn DeployStrategy> {
    match target {
        DeployTarget::Local => Box::new(LocalDeploy),
        DeployTarget::Cloud => Box::new(CloudDeploy),
        DeployTarget::Server => Box::new(ServerDeploy),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// LocalDeploy — docker compose up/down
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct LocalDeploy;

impl DeployStrategy for LocalDeploy {
    fn validate(&self, _config: &StackerConfig) -> Result<(), CliError> {
        // Local deploy only requires Docker to be available;
        // that check happens at command level before calling deploy().
        Ok(())
    }

    fn deploy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<DeployResult, CliError> {
        let compose_path = context.compose_path.to_string_lossy().to_string();

        let mut args: Vec<String> = vec!["compose".into()];
        if let Some(ref env_file) = config.env_file {
            let env_file_path = if env_file.is_absolute() {
                env_file.clone()
            } else {
                context.project_dir.join(env_file)
            };
            args.push("--env-file".into());
            args.push(env_file_path.to_string_lossy().to_string());
        }
        args.push("-f".into());
        args.push(compose_path.clone());

        if context.dry_run {
            args.push("config".into());
        } else {
            args.push("up".into());
            args.push("-d".into());
            args.push("--build".into());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = executor.execute("docker", &args_refs)?;

        if !output.stdout.trim().is_empty() {
            println!("{}", output.stdout);
        }
        if !output.stderr.trim().is_empty() {
            eprintln!("{}", output.stderr);
        }

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Local,
                reason: format!("docker compose failed: {}", output.stderr.trim()),
            });
        }

        let action = if context.dry_run { "validated" } else { "started" };
        Ok(DeployResult {
            target: DeployTarget::Local,
            message: format!("Local deployment {} successfully", action),
            server_ip: None,
        })
    }

    fn destroy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<(), CliError> {
        let compose_path = context.compose_path.to_string_lossy().to_string();
        let mut args: Vec<String> = vec!["compose".into()];
        if let Some(ref env_file) = config.env_file {
            let env_file_path = if env_file.is_absolute() {
                env_file.clone()
            } else {
                context.project_dir.join(env_file)
            };
            args.push("--env-file".into());
            args.push(env_file_path.to_string_lossy().to_string());
        }
        args.push("-f".into());
        args.push(compose_path);
        args.push("down".into());
        args.push("--volumes".into());
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = executor.execute("docker", &args_refs)?;

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Local,
                reason: format!("docker compose down failed: {}", output.stderr.trim()),
            });
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// InstallContainerCommand — builds `docker run` for the install container
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Builder for `docker run` commands that launch the install container
/// with Terraform + Ansible for cloud/server deployments.
///
/// Modeled after the existing install service docker-compose mounts
/// and the `ConfigureProxyCommandRequest` pattern in `forms/status_panel.rs`.
#[derive(Debug, Clone)]
pub struct InstallContainerCommand {
    image: String,
    volume_mounts: Vec<(String, String)>,
    env_vars: Vec<(String, String)>,
    action: InstallAction,
    remove_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallAction {
    Plan,
    Apply,
    Destroy,
}

impl InstallAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
            Self::Destroy => "destroy",
        }
    }
}

impl InstallContainerCommand {
    /// Create a new builder with the given image (or default).
    pub fn new(image: Option<&str>) -> Self {
        Self {
            image: image.unwrap_or(DEFAULT_INSTALL_IMAGE).to_string(),
            volume_mounts: Vec::new(),
            env_vars: Vec::new(),
            action: InstallAction::Apply,
            remove_after: true,
        }
    }

    /// Mount a host path into the container.
    pub fn mount(mut self, host_path: impl AsRef<Path>, container_path: &str) -> Self {
        self.volume_mounts.push((
            host_path.as_ref().to_string_lossy().to_string(),
            container_path.to_string(),
        ));
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the action to perform (plan, apply, destroy).
    pub fn action(mut self, action: InstallAction) -> Self {
        self.action = action;
        self
    }

    /// Whether to remove the container after exit (--rm). Default: true.
    pub fn remove_after(mut self, remove: bool) -> Self {
        self.remove_after = remove;
        self
    }

    /// Build the argument list for `docker run`.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec!["run".to_string()];

        if self.remove_after {
            args.push("--rm".to_string());
        }

        for (host, container) in &self.volume_mounts {
            args.push("-v".to_string());
            args.push(format!("{}:{}", host, container));
        }

        for (key, value) in &self.env_vars {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(self.image.clone());
        args.push(self.action.as_str().to_string());

        args
    }

    /// Build from a `StackerConfig` and `DeployContext`, setting up
    /// standard mounts and environment variables.
    pub fn from_config(
        config: &StackerConfig,
        context: &DeployContext,
        action: InstallAction,
    ) -> Self {
        let mut cmd = Self::new(Some(context.install_image())).action(action);

        // Mount stacker.yml
        cmd = cmd.mount(&context.config_path, CONTAINER_CONFIG_PATH);

        // Mount compose file
        cmd = cmd.mount(&context.compose_path, CONTAINER_COMPOSE_PATH);

        // Set project name
        cmd = cmd.env("PROJECT_NAME", &config.name);

        // Cloud-specific configuration
        if let Some(ref cloud) = config.deploy.cloud {
            cmd = cmd.env("CLOUD_PROVIDER", &cloud.provider.to_string());

            if let Some(ref region) = cloud.region {
                cmd = cmd.env("CLOUD_REGION", region);
            }

            if let Some(ref size) = cloud.size {
                cmd = cmd.env("CLOUD_SIZE", size);
            }

            // Mount SSH key if specified
            if let Some(ref ssh_key) = cloud.ssh_key {
                cmd = cmd.mount(ssh_key, CONTAINER_SSH_KEY_PATH);
            }
        }

        // Server-specific configuration
        if let Some(ref server) = config.deploy.server {
            cmd = cmd.env("SERVER_HOST", &server.host);
            cmd = cmd.env("SERVER_USER", &server.user);
            cmd = cmd.env("SERVER_PORT", &server.port.to_string());

            if let Some(ref ssh_key) = server.ssh_key {
                cmd = cmd.mount(ssh_key, CONTAINER_SSH_KEY_PATH);
            }
        }

        cmd
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CloudDeploy — install container with Terraform/Ansible
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct CloudDeploy;

impl DeployStrategy for CloudDeploy {
    fn validate(&self, config: &StackerConfig) -> Result<(), CliError> {
        if config.deploy.cloud.is_none() {
            return Err(CliError::CloudProviderMissing);
        }

        Ok(())
    }

    fn deploy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<DeployResult, CliError> {
        if let Some(cloud_cfg) = &config.deploy.cloud {
            if cloud_cfg.orchestrator == CloudOrchestrator::Remote {
                let cred_manager = CredentialsManager::with_default_store();
                let creds = cred_manager.require_valid_token("remote cloud orchestrator deployment")?;

                if context.dry_run {
                    return Ok(DeployResult {
                        target: DeployTarget::Cloud,
                        message: "Remote cloud deploy dry-run validated payload and credentials".to_string(),
                        server_ip: None,
                    });
                }

                // Resolve project name: CLI flag > config project.identity > config name
                let project_name = context
                    .project_name_override
                    .clone()
                    .or_else(|| config.project.identity.clone())
                    .unwrap_or_else(|| config.name.clone());

                // Resolve cloud key name: CLI flag > config deploy.cloud.key
                let key_name = context
                    .key_name_override
                    .clone()
                    .or_else(|| cloud_cfg.key.clone());

                // Resolve server name: CLI flag > config deploy.cloud.server
                let server_name = context
                    .server_name_override
                    .clone()
                    .or_else(|| cloud_cfg.server.clone());

                let base_url = normalize_stacker_server_url(
                    stacker_client::DEFAULT_STACKER_URL,
                );

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| CliError::DeployFailed {
                        target: DeployTarget::Cloud,
                        reason: format!("Failed to initialize async runtime: {}", e),
                    })?;

                let response = rt.block_on(async {
                    let client = StackerClient::new(&base_url, &creds.access_token);

                    // Step 1: Resolve or auto-create project
                    eprintln!("  Resolving project '{}'...", project_name);
                    let project_body = stacker_client::build_project_body(config);
                    let project = match client.find_project_by_name(&project_name).await? {
                        Some(p) => {
                            eprintln!("  Found project '{}' (id={}), syncing metadata...", p.name, p.id);
                            let updated = client
                                .update_project(p.id, project_body)
                                .await?;
                            eprintln!("  Updated project '{}' (id={})", updated.name, updated.id);
                            updated
                        }
                        None => {
                            eprintln!("  Project '{}' not found, creating...", project_name);
                            let p = client
                                .create_project(&project_name, project_body)
                                .await?;
                            eprintln!("  Created project '{}' (id={})", p.name, p.id);
                            p
                        }
                    };

                    // Step 2: Resolve cloud credentials
                    let cloud_id = if let Some(key_ref) = &key_name {
                        // Look up saved cloud by provider name
                        eprintln!("  Looking up saved cloud key '{}'...", key_ref);
                        match client.find_cloud_by_provider(key_ref).await? {
                            Some(c) => {
                                eprintln!(
                                    "  Found cloud credentials (id={}, provider={})",
                                    c.id, c.provider
                                );
                                Some(c.id)
                            }
                            None => {
                                // Try saving current env-var creds under this provider
                                let provider_str = cloud_cfg.provider.to_string();
                                let provider_code = provider_code_for_remote(
                                    &provider_str,
                                );
                                let env_creds =
                                    resolve_remote_cloud_credentials(provider_code);
                                let cloud_token = env_creds
                                    .get("cloud_token")
                                    .and_then(|v| v.as_str());
                                let cloud_key = env_creds
                                    .get("cloud_key")
                                    .and_then(|v| v.as_str());
                                let cloud_secret = env_creds
                                    .get("cloud_secret")
                                    .and_then(|v| v.as_str());

                                if cloud_token.is_some()
                                    || cloud_key.is_some()
                                    || cloud_secret.is_some()
                                {
                                    eprintln!(
                                        "  No saved cloud '{}', saving from env vars...",
                                        key_ref
                                    );
                                    let saved = client
                                        .save_cloud(
                                            provider_code,
                                            cloud_token,
                                            cloud_key,
                                            cloud_secret,
                                        )
                                        .await?;
                                    eprintln!(
                                        "  Saved cloud credentials (id={})",
                                        saved.id
                                    );
                                    Some(saved.id)
                                } else {
                                    return Err(CliError::DeployFailed {
                                        target: DeployTarget::Cloud,
                                        reason: format!(
                                            "Cloud key '{}' not found on server and no cloud credentials in env vars (STACKER_CLOUD_TOKEN, HCLOUD_TOKEN, etc.)",
                                            key_ref
                                        ),
                                    });
                                }
                            }
                        }
                    } else {
                        // No key specified: try to find existing cloud creds for this provider,
                        // or pass creds directly in deploy form from env vars
                        let provider_str = cloud_cfg.provider.to_string();
                        let provider_code =
                            provider_code_for_remote(&provider_str);
                        match client.find_cloud_by_provider(provider_code).await? {
                            Some(c) => {
                                eprintln!(
                                    "  Using saved cloud credentials (id={}, provider={})",
                                    c.id, c.provider
                                );
                                Some(c.id)
                            }
                            None => None,
                        }
                    };

                    // Step 3: Resolve server by name
                    let server_id = if let Some(srv_name) = &server_name {
                        eprintln!("  Looking up server '{}'...", srv_name);
                        match client.find_server_by_name(srv_name).await? {
                            Some(s) => {
                                eprintln!(
                                    "  Found server '{}' (id={})",
                                    s.name.as_deref().unwrap_or("unnamed"),
                                    s.id
                                );
                                Some(s.id)
                            }
                            None => {
                                return Err(CliError::DeployFailed {
                                    target: DeployTarget::Cloud,
                                    reason: format!(
                                        "Server '{}' not found. Create it on the Stacker server first or remove --server flag.",
                                        srv_name
                                    ),
                                });
                            }
                        }
                    } else {
                        None
                    };

                    // Step 4: Build deploy form
                    let mut deploy_form = stacker_client::build_deploy_form(config);
                    if let Some(sid) = server_id {
                        if let Some(server_obj) = deploy_form.get_mut("server") {
                            if let Some(obj) = server_obj.as_object_mut() {
                                obj.insert(
                                    "server_id".to_string(),
                                    serde_json::Value::Number(sid.into()),
                                );
                            }
                        }
                    }

                    // Include env-var cloud creds in form if no saved cloud
                    if cloud_id.is_none() {
                        let provider_str = cloud_cfg.provider.to_string();
                        let provider_code =
                            provider_code_for_remote(&provider_str);
                        let env_creds = resolve_remote_cloud_credentials(provider_code);
                        if let Some(cloud_obj) = deploy_form.get_mut("cloud") {
                            if let Some(obj) = cloud_obj.as_object_mut() {
                                for (k, v) in &env_creds {
                                    obj.insert(k.clone(), v.clone());
                                }
                                obj.insert(
                                    "save_token".to_string(),
                                    serde_json::Value::Bool(true),
                                );
                            }
                        }
                    }

                    // Step 5: Deploy
                    eprintln!("  Deploying project '{}' (id={})...", project_name, project.id);
                    let resp = client.deploy(project.id, cloud_id, deploy_form).await?;

                    Ok(resp)
                }).map_err(|e: CliError| e)?;

                let deploy_id = response
                    .meta
                    .as_ref()
                    .and_then(|m| m.get("deployment_id"))
                    .and_then(|v| v.as_i64());

                let project_id = response.id;

                let mut message = format!(
                    "Cloud deployment requested via Stacker server (project='{}'",
                    project_name
                );

                if let Some(pid) = project_id {
                    message.push_str(&format!(", project_id={}", pid));
                }
                if let Some(did) = deploy_id {
                    message.push_str(&format!(", deployment_id={}", did));
                }
                message.push(')');

                if let Some(srv) = &server_name {
                    message.push_str(&format!("; server='{}'", srv));
                }
                if let Some(key) = &key_name {
                    message.push_str(&format!("; cloud_key='{}'", key));
                }

                return Ok(DeployResult {
                    target: DeployTarget::Cloud,
                    message,
                    server_ip: None,
                });
            }
        }

        let action = if context.dry_run {
            InstallAction::Plan
        } else {
            InstallAction::Apply
        };

        let cmd = InstallContainerCommand::from_config(config, context, action);
        let args = cmd.build_args();
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = executor.execute("docker", &args_refs)?;

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!("Install container failed: {}", output.stderr.trim()),
            });
        }

        let action_str = if context.dry_run { "plan completed" } else { "deployed" };
        Ok(DeployResult {
            target: DeployTarget::Cloud,
            message: format!("Cloud deployment {}", action_str),
            server_ip: extract_server_ip(&output.stdout),
        })
    }

    fn destroy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<(), CliError> {
        if let Some(cloud_cfg) = &config.deploy.cloud {
            if cloud_cfg.orchestrator == CloudOrchestrator::Remote {
                return Err(CliError::DeployFailed {
                    target: DeployTarget::Cloud,
                    reason: "Remote cloud orchestrator destroy is not implemented yet. Use platform API/UI or switch deploy.cloud.orchestrator=local.".to_string(),
                });
            }
        }

        let cmd = InstallContainerCommand::from_config(config, context, InstallAction::Destroy);
        let args = cmd.build_args();
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = executor.execute("docker", &args_refs)?;

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!("Cloud destroy failed: {}", output.stderr.trim()),
            });
        }

        Ok(())
    }
}

pub fn provider_code_for_remote(config_provider: &str) -> &str {
    match config_provider {
        "hetzner" => "htz",
        "digitalocean" => "do",
        "aws" => "aws",
        "linode" => "lo",
        "vultr" => "vu",
        _ => config_provider,
    }
}

#[allow(dead_code)]
fn normalize_user_service_base_url(raw: &str) -> String {
    let mut url = raw.trim_end_matches('/').to_string();
    if url.ends_with("/server/user/auth/login") {
        let len = url.len() - "/auth/login".len();
        return url[..len].to_string();
    }

    for suffix in ["/oauth_server/token", "/auth/login", "/login"] {
        if url.ends_with(suffix) {
            let len = url.len() - suffix.len();
            url = url[..len].to_string();
            break;
        }
    }
    url
}

/// Normalize the Stacker server URL from stored credentials.
/// Strips trailing slashes and known auth path suffixes to get the base API URL.
fn normalize_stacker_server_url(raw: &str) -> String {
    let mut url = raw.trim_end_matches('/').to_string();
    // Strip known auth endpoints that might be stored as server_url
    for suffix in ["/oauth_server/token", "/auth/login", "/server/user/auth/login", "/login", "/api"] {
        if url.ends_with(suffix) {
            let len = url.len() - suffix.len();
            url = url[..len].to_string();
            break;
        }
    }
    url
}

#[allow(dead_code)]
fn sanitize_stack_code(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "app-stack".to_string()
    } else {
        out
    }
}

#[allow(dead_code)]
fn default_common_domain(project_name: &str) -> String {
    format!("{}.example.com", sanitize_stack_code(project_name))
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn resolve_remote_cloud_credentials(provider: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut creds = serde_json::Map::new();

    match provider {
        "htz" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_HETZNER_TOKEN",
                "HETZNER_TOKEN",
                "HCLOUD_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "do" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_DIGITALOCEAN_TOKEN",
                "DIGITALOCEAN_TOKEN",
                "DO_API_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "lo" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_LINODE_TOKEN",
                "LINODE_TOKEN",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "vu" => {
            if let Some(token) = first_non_empty_env(&[
                "STACKER_CLOUD_TOKEN",
                "STACKER_VULTR_TOKEN",
                "VULTR_TOKEN",
                "VULTR_API_KEY",
            ]) {
                creds.insert("cloud_token".to_string(), serde_json::Value::String(token));
            }
        }
        "aws" => {
            if let Some(key) = first_non_empty_env(&["STACKER_CLOUD_KEY", "AWS_ACCESS_KEY_ID"]) {
                creds.insert("cloud_key".to_string(), serde_json::Value::String(key));
            }
            if let Some(secret) =
                first_non_empty_env(&["STACKER_CLOUD_SECRET", "AWS_SECRET_ACCESS_KEY"])
            {
                creds.insert("cloud_secret".to_string(), serde_json::Value::String(secret));
            }
        }
        _ => {}
    }

    creds
}

#[allow(dead_code)]
fn build_remote_deploy_payload(config: &StackerConfig) -> serde_json::Value {
    let cloud = config.deploy.cloud.as_ref();
    let provider = cloud
        .map(|c| provider_code_for_remote(&c.provider.to_string()).to_string())
        .unwrap_or_else(|| "htz".to_string());
    let region = cloud.and_then(|c| c.region.clone()).unwrap_or_else(|| "nbg1".to_string());
    let server = cloud.and_then(|c| c.size.clone()).unwrap_or_else(|| "cx11".to_string());
    let stack_code = config
        .project
        .identity
        .clone()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "custom-stack".to_string());
    let os = match provider.as_str() {
        "do" => "docker-20-04",
        _ => "ubuntu-22.04",
    };

    let mut payload = serde_json::json!({
        "provider": provider,
        "region": region,
        "server": server,
        "os": os,
        "ssl": "letsencrypt",
        "commonDomain": default_common_domain(&config.name),
        "domainList": {},
        "stack_code": stack_code,
        "project_name": config.name,
        "selected_plan": "free",
        "payment_type": "subscription",
        "subscriptions": [],
        "vars": [],
        "integrated_features": [],
        "extended_features": [],
        "save_token": true,
        "custom": {
            "project_name": config.name,
            "custom_stack_code": sanitize_stack_code(&config.name),
            "project_overview": format!("Generated by stacker-cli for {}", config.name)
        }
    });

    if let Some(obj) = payload.as_object_mut() {
        for (key, value) in resolve_remote_cloud_credentials(&provider) {
            obj.insert(key, value);
        }
    }

    payload
}

#[allow(dead_code)]
fn validate_remote_deploy_payload(payload: &serde_json::Value) -> Result<(), CliError> {
    let required = [
        "provider",
        "region",
        "server",
        "os",
        "commonDomain",
        "stack_code",
        "selected_plan",
        "payment_type",
        "subscriptions",
    ];

    let mut missing = Vec::new();

    for key in required {
        match payload.get(key) {
            Some(v) if !v.is_null() => {
                if key == "subscriptions" && !v.is_array() {
                    missing.push("subscriptions(array)");
                }
                if key == "stack_code"
                    && v
                        .as_str()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    missing.push("stack_code(non-empty)");
                }
            }
            _ => missing.push(key),
        }
    }

    if !missing.is_empty() {
        let identity_hint = if missing
            .iter()
            .any(|item| item.contains("stack_code"))
        {
            " stack_code defaults to 'custom-stack'. Optionally set project.identity in stacker.yml to a registered catalog stack code."
        } else {
            ""
        };
        Err(CliError::DeployFailed {
            target: DeployTarget::Cloud,
            reason: format!(
                "Remote deploy payload is missing required fields: {}. Preferred flow: remove `deploy.cloud.remote_payload_file` and run `stacker deploy --target cloud` so payload is generated automatically. For advanced/debug use `stacker-cli config setup remote-payload`.{}",
                missing.join(", "),
                identity_hint
            ),
        })
    } else {
        let mut invalid = Vec::new();

        if let Some(domain) = payload.get("commonDomain").and_then(|v| v.as_str()) {
            let normalized = domain.trim().to_ascii_lowercase();
            if normalized == "localhost" || !normalized.contains('.') {
                invalid.push("commonDomain(valid domain required)");
            }
        }

        let provider = payload
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        match provider {
            "htz" | "lo" | "vu" => {
                let has_token = payload
                    .get("cloud_token")
                    .and_then(|v| v.as_str())
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
                if !has_token {
                    invalid.push("cloud_token");
                }
            }
            "aws" => {
                let has_key = payload
                    .get("cloud_key")
                    .and_then(|v| v.as_str())
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
                let has_secret = payload
                    .get("cloud_secret")
                    .and_then(|v| v.as_str())
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);

                if !has_key {
                    invalid.push("cloud_key");
                }
                if !has_secret {
                    invalid.push("cloud_secret");
                }
            }
            _ => {}
        }

        if invalid.is_empty() {
            Ok(())
        } else {
            Err(CliError::DeployFailed {
                target: DeployTarget::Cloud,
                reason: format!(
                    "Remote deploy payload has invalid/missing provider credentials: {}. Set env vars before deploy (e.g. STACKER_CLOUD_TOKEN or provider-specific token vars; for AWS use AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY).",
                    invalid.join(", ")
                ),
            })
        }
    }
}

#[allow(dead_code)]
fn persist_remote_payload_snapshot(project_dir: &Path, payload: &serde_json::Value) -> Option<PathBuf> {
    let stacker_dir = project_dir.join(".stacker");
    let snapshot_path = stacker_dir.join("remote-payload.last.json");

    if let Err(err) = std::fs::create_dir_all(&stacker_dir) {
        eprintln!(
            "Warning: failed to create payload snapshot directory {}: {}",
            stacker_dir.display(),
            err
        );
        return None;
    }

    let payload_str = match serde_json::to_string_pretty(payload) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("Warning: failed to serialize remote payload snapshot: {}", err);
            return None;
        }
    };

    if let Err(err) = std::fs::write(&snapshot_path, payload_str) {
        eprintln!(
            "Warning: failed to write payload snapshot {}: {}",
            snapshot_path.display(),
            err
        );
        return None;
    }

    Some(snapshot_path)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ServerDeploy — SSH + install container
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct ServerDeploy;

impl DeployStrategy for ServerDeploy {
    fn validate(&self, config: &StackerConfig) -> Result<(), CliError> {
        if config.deploy.server.is_none() {
            return Err(CliError::ServerHostMissing);
        }

        Ok(())
    }

    fn deploy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<DeployResult, CliError> {
        let action = if context.dry_run {
            InstallAction::Plan
        } else {
            InstallAction::Apply
        };

        let cmd = InstallContainerCommand::from_config(config, context, action);
        let args = cmd.build_args();
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = executor.execute("docker", &args_refs)?;

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Server,
                reason: format!("Server deployment failed: {}", output.stderr.trim()),
            });
        }

        let server_host = config
            .deploy
            .server
            .as_ref()
            .map(|s| s.host.clone());

        let action_str = if context.dry_run { "plan completed" } else { "deployed" };
        Ok(DeployResult {
            target: DeployTarget::Server,
            message: format!("Server deployment {}", action_str),
            server_ip: server_host,
        })
    }

    fn destroy(
        &self,
        config: &StackerConfig,
        context: &DeployContext,
        executor: &dyn CommandExecutor,
    ) -> Result<(), CliError> {
        let cmd = InstallContainerCommand::from_config(config, context, InstallAction::Destroy);
        let args = cmd.build_args();
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let output = executor.execute("docker", &args_refs)?;

        if !output.success() {
            return Err(CliError::DeployFailed {
                target: DeployTarget::Server,
                reason: format!("Server destroy failed: {}", output.stderr.trim()),
            });
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Try to extract a server IP from install container stdout.
/// Looks for lines like `server_ip = 1.2.3.4` (Terraform output format).
fn extract_server_ip(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("server_ip") || trimmed.starts_with("public_ip") {
            if let Some(value) = trimmed.split('=').nth(1) {
                let ip = value.trim().trim_matches('"');
                if !ip.is_empty() {
                    return Some(ip.to_string());
                }
            }
        }
    }
    None
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use crate::cli::config_parser::{CloudConfig, CloudOrchestrator, CloudProvider, ConfigBuilder, ServerConfig};

    // ── Mock executor ───────────────────────────────

    struct MockExecutor {
        recorded_calls: Mutex<Vec<(String, Vec<String>)>>,
        exit_code: i32,
        stdout: String,
        stderr: String,
    }

    impl MockExecutor {
        fn success() -> Self {
            Self {
                recorded_calls: Mutex::new(Vec::new()),
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        }

        #[allow(dead_code)]
        fn success_with_stdout(stdout: &str) -> Self {
            Self {
                recorded_calls: Mutex::new(Vec::new()),
                exit_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            }
        }

        fn failure(stderr: &str) -> Self {
            Self {
                recorded_calls: Mutex::new(Vec::new()),
                exit_code: 1,
                stdout: String::new(),
                stderr: stderr.to_string(),
            }
        }

        fn last_call(&self) -> (String, Vec<String>) {
            self.recorded_calls.lock().unwrap().last().cloned().unwrap()
        }

        fn last_args(&self) -> Vec<String> {
            self.last_call().1
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CliError> {
            self.recorded_calls.lock().unwrap().push((
                program.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));

            Ok(CommandOutput {
                exit_code: self.exit_code,
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            })
        }
    }

    // Helper to join args as a single string for easier assertion.
    fn args_as_string(args: &[String]) -> String {
        args.join(" ")
    }

    fn sample_cloud_config() -> StackerConfig {
        ConfigBuilder::new().name("test-cloud-app")
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Local,
                region: Some("fsn1".to_string()),
                size: Some("cx21".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
                key: None,
                server: None,
            })
            .build()
            .unwrap()
    }

    #[test]
    fn test_normalize_user_service_base_url_from_token_endpoint() {
        let url = normalize_user_service_base_url("https://api.try.direct/oauth_server/token");
        assert_eq!(url, "https://api.try.direct");
    }

    #[test]
    fn test_normalize_user_service_base_url_from_direct_login_endpoint() {
        let url = normalize_user_service_base_url("https://dev.try.direct/server/user/auth/login");
        assert_eq!(url, "https://dev.try.direct/server/user");
    }

    #[test]
    fn test_provider_code_for_remote_hetzner() {
        assert_eq!(provider_code_for_remote("hetzner"), "htz");
        assert_eq!(provider_code_for_remote("aws"), "aws");
        assert_eq!(provider_code_for_remote("linode"), "lo");
        assert_eq!(provider_code_for_remote("vultr"), "vu");
    }

    #[test]
    fn test_build_remote_deploy_payload_contains_required_fields() {
        let cfg = sample_cloud_config();
        let payload = build_remote_deploy_payload(&cfg);
        assert!(payload.get("provider").is_some());
        assert!(payload.get("region").is_some());
        assert!(payload.get("server").is_some());
        assert!(payload.get("os").is_some());
        assert!(payload.get("commonDomain").is_some());
        assert!(payload.get("selected_plan").is_some());
        assert!(payload.get("payment_type").is_some());
        assert!(payload.get("subscriptions").is_some());
        assert!(payload.get("stack_code").is_some());
        assert_eq!(
            payload.get("stack_code").and_then(|v| v.as_str()),
            Some("custom-stack")
        );
    }

    #[test]
    fn test_build_remote_deploy_payload_uses_project_identity_when_set() {
        let cfg = ConfigBuilder::new()
            .name("test-cloud-app")
            .project_identity("registered-stack-code")
            .deploy_target(DeployTarget::Cloud)
            .cloud(CloudConfig {
                provider: CloudProvider::Hetzner,
                orchestrator: CloudOrchestrator::Local,
                region: Some("fsn1".to_string()),
                size: Some("cx21".to_string()),
                install_image: None,
                remote_payload_file: None,
                ssh_key: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
                key: None,
                server: None,
            })
            .build()
            .unwrap();

        let payload = build_remote_deploy_payload(&cfg);
        assert_eq!(
            payload.get("stack_code").and_then(|v| v.as_str()),
            Some("registered-stack-code")
        );
    }

    #[test]
    fn test_validate_remote_deploy_payload_accepts_generated_payload() {
        std::env::set_var("STACKER_CLOUD_TOKEN", "test-token-value");
        let cfg = sample_cloud_config();
        let payload = build_remote_deploy_payload(&cfg);
        let result = validate_remote_deploy_payload(&payload);
        std::env::remove_var("STACKER_CLOUD_TOKEN");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_remote_deploy_payload_rejects_missing_common_domain() {
        let payload = serde_json::json!({
            "provider": "htz",
            "region": "nbg1",
            "server": "cx11",
            "os": "ubuntu-22.04",
            "stack_code": "demo",
            "selected_plan": "free",
            "payment_type": "subscription",
            "subscriptions": []
        });

        let err = validate_remote_deploy_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("commonDomain"));
    }

    #[test]
    fn test_validate_remote_deploy_payload_rejects_empty_stack_code() {
        let payload = serde_json::json!({
            "provider": "htz",
            "region": "nbg1",
            "server": "cx11",
            "os": "ubuntu-22.04",
            "commonDomain": "example.com",
            "stack_code": "",
            "selected_plan": "free",
            "payment_type": "subscription",
            "subscriptions": [],
            "cloud_token": "token"
        });

        let err = validate_remote_deploy_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("stack_code"));
    }

    #[test]
    fn test_persist_remote_payload_snapshot_writes_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let payload = serde_json::json!({
            "provider": "htz",
            "region": "nbg1",
            "server": "cx11",
            "os": "ubuntu-22.04",
            "commonDomain": "localhost",
            "stack_code": "demo-stack",
            "selected_plan": "free",
            "payment_type": "subscription",
            "subscriptions": []
        });

        let path = persist_remote_payload_snapshot(dir.path(), &payload).unwrap();
        assert!(path.exists());

        let raw = std::fs::read_to_string(path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.get("provider").and_then(|v| v.as_str()), Some("htz"));
    }

    fn sample_server_config() -> StackerConfig {
        ConfigBuilder::new().name("test-server-app")
            .deploy_target(DeployTarget::Server)
            .server(ServerConfig {
                host: "192.168.1.100".to_string(),
                user: "deploy".to_string(),
                ssh_key: Some(PathBuf::from("/home/user/.ssh/id_rsa")),
                port: 22,
            })
            .build()
            .unwrap()
    }

    fn sample_context(dry_run: bool) -> DeployContext {
        DeployContext {
            config_path: PathBuf::from("/project/stacker.yml"),
            compose_path: PathBuf::from("/project/docker-compose.yml"),
            project_dir: PathBuf::from("/project"),
            dry_run,
            image: None,
            project_name_override: None,
            key_name_override: None,
            server_name_override: None,
        }
    }

    // ── Phase 6 tests ───────────────────────────────

    #[test]
    fn test_build_run_command_with_cloud_config() {
        let config = sample_cloud_config();
        let context = sample_context(false);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Apply);
        let args = args_as_string(&cmd.build_args());

        assert!(args.contains("-v /project/stacker.yml:/app/stacker.yml"));
        assert!(args.contains("-v /project/docker-compose.yml:/app/docker-compose.yml"));
        assert!(args.contains("-e CLOUD_PROVIDER=hetzner"));
        assert!(args.contains("-e CLOUD_REGION=fsn1"));
        assert!(args.contains("-e PROJECT_NAME=test-cloud-app"));
    }

    #[test]
    fn test_run_command_mounts_stacker_yml() {
        let config = sample_cloud_config();
        let context = sample_context(false);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Apply);
        let args = cmd.build_args();

        let mount_idx = args.iter().position(|a| a == "-v").unwrap();
        let mount_val = &args[mount_idx + 1];
        assert!(mount_val.contains("stacker.yml:/app/stacker.yml"));
    }

    #[test]
    fn test_run_command_mounts_ssh_key() {
        let config = sample_cloud_config();
        let context = sample_context(false);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Apply);
        let args = args_as_string(&cmd.build_args());

        assert!(args.contains("-v /home/user/.ssh/id_ed25519:/root/.ssh/id_rsa"));
    }

    #[test]
    fn test_run_command_plan_mode() {
        let config = sample_cloud_config();
        let context = sample_context(true);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Plan);
        let args = cmd.build_args();

        let last = args.last().unwrap();
        assert_eq!(last, "plan");
        assert!(!args.contains(&"apply".to_string()));
    }

    #[test]
    fn test_run_command_apply_mode() {
        let config = sample_cloud_config();
        let context = sample_context(false);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Apply);
        let args = cmd.build_args();

        let last = args.last().unwrap();
        assert_eq!(last, "apply");
        assert!(!args.contains(&"plan".to_string()));
    }

    #[test]
    fn test_install_container_image_tag() {
        let cmd = InstallContainerCommand::new(None);
        let args = cmd.build_args();

        assert!(args.contains(&DEFAULT_INSTALL_IMAGE.to_string()));
    }

    // ── Additional tests ────────────────────────────

    #[test]
    fn test_install_container_custom_image() {
        let cmd = InstallContainerCommand::new(Some("custom/installer:v2"));
        let args = cmd.build_args();

        assert!(args.contains(&"custom/installer:v2".to_string()));
        assert!(!args.contains(&DEFAULT_INSTALL_IMAGE.to_string()));
    }

    #[test]
    fn test_deploy_context_default_image() {
        let ctx = sample_context(false);
        assert_eq!(ctx.install_image(), DEFAULT_INSTALL_IMAGE);
    }

    #[test]
    fn test_deploy_context_custom_image() {
        let ctx = DeployContext {
            config_path: PathBuf::from("/p/stacker.yml"),
            compose_path: PathBuf::from("/p/docker-compose.yml"),
            project_dir: PathBuf::from("/p"),
            dry_run: false,
            image: Some("mycompany/install:v3".to_string()),
            project_name_override: None,
            key_name_override: None,
            server_name_override: None,
        };
        assert_eq!(ctx.install_image(), "mycompany/install:v3");
    }

    #[test]
    fn test_local_deploy_dry_run() {
        let config = ConfigBuilder::new().name("local-app").build().unwrap();
        let context = sample_context(true);
        let executor = MockExecutor::success();
        let strategy = LocalDeploy;

        let result = strategy.deploy(&config, &context, &executor).unwrap();
        assert_eq!(result.target, DeployTarget::Local);
        assert!(result.message.contains("validated"));

        let args = executor.last_args();
        assert!(args.contains(&"config".to_string()));
        assert!(!args.contains(&"up".to_string()));
    }

    #[test]
    fn test_local_deploy_apply() {
        let config = ConfigBuilder::new().name("local-app").build().unwrap();
        let context = sample_context(false);
        let executor = MockExecutor::success();
        let strategy = LocalDeploy;

        let result = strategy.deploy(&config, &context, &executor).unwrap();
        assert_eq!(result.target, DeployTarget::Local);
        assert!(result.message.contains("started"));

        let args = executor.last_args();
        assert!(args.contains(&"up".to_string()));
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"--build".to_string()));
    }

    #[test]
    fn test_local_deploy_failure() {
        let config = ConfigBuilder::new().name("local-app").build().unwrap();
        let context = sample_context(false);
        let executor = MockExecutor::failure("service failed to start");
        let strategy = LocalDeploy;

        let result = strategy.deploy(&config, &context, &executor);
        assert!(result.is_err());
    }

    #[test]
    fn test_local_destroy() {
        let config = ConfigBuilder::new().name("local-app").build().unwrap();
        let context = sample_context(false);
        let executor = MockExecutor::success();
        let strategy = LocalDeploy;

        strategy.destroy(&config, &context, &executor).unwrap();

        let args = executor.last_args();
        assert!(args.contains(&"down".to_string()));
        assert!(args.contains(&"--volumes".to_string()));
    }

    #[test]
    fn test_local_deploy_uses_env_file_when_configured() {
        let config = ConfigBuilder::new()
            .name("local-app")
            .env_file(".env")
            .build()
            .unwrap();
        let context = sample_context(true);
        let executor = MockExecutor::success();
        let strategy = LocalDeploy;

        strategy.deploy(&config, &context, &executor).unwrap();

        let args = executor.last_args();
        assert!(args.contains(&"--env-file".to_string()));
        assert!(args.contains(&"/project/.env".to_string()));
    }

    #[test]
    fn test_cloud_deploy_validates_provider() {
        let config = ConfigBuilder::new().name("no-cloud").build().unwrap();
        let strategy = CloudDeploy;
        let result = strategy.validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_cloud_deploy_has_provider_passes() {
        let config = sample_cloud_config();
        let strategy = CloudDeploy;
        assert!(strategy.validate(&config).is_ok());
    }

    #[test]
    fn test_cloud_deploy_runs_install_container() {
        let config = sample_cloud_config();
        let context = sample_context(false);
        let executor = MockExecutor::success();
        let strategy = CloudDeploy;

        let result = strategy.deploy(&config, &context, &executor).unwrap();
        assert_eq!(result.target, DeployTarget::Cloud);

        let (program, args) = executor.last_call();
        assert_eq!(program, "docker");
        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&DEFAULT_INSTALL_IMAGE.to_string()));
        assert!(args.contains(&"apply".to_string()));
    }

    #[test]
    fn test_cloud_deploy_dry_run_uses_plan() {
        let config = sample_cloud_config();
        let context = sample_context(true);
        let executor = MockExecutor::success();
        let strategy = CloudDeploy;

        strategy.deploy(&config, &context, &executor).unwrap();

        let args = executor.last_args();
        assert!(args.contains(&"plan".to_string()));
        assert!(!args.contains(&"apply".to_string()));
    }

    #[test]
    fn test_server_deploy_validates_host() {
        let config = ConfigBuilder::new().name("no-server").build().unwrap();
        let strategy = ServerDeploy;
        let result = strategy.validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_deploy_has_host_passes() {
        let config = sample_server_config();
        let strategy = ServerDeploy;
        assert!(strategy.validate(&config).is_ok());
    }

    #[test]
    fn test_server_deploy_sets_env_vars() {
        let config = sample_server_config();
        let context = sample_context(false);
        let cmd = InstallContainerCommand::from_config(&config, &context, InstallAction::Apply);
        let args = args_as_string(&cmd.build_args());

        assert!(args.contains("-e SERVER_HOST=192.168.1.100"));
        assert!(args.contains("-e SERVER_USER=deploy"));
        assert!(args.contains("-e SERVER_PORT=22"));
    }

    #[test]
    fn test_extract_server_ip_from_terraform_output() {
        let stdout = "Apply complete!\n\nOutputs:\n\nserver_ip = \"203.0.113.42\"\n";
        assert_eq!(extract_server_ip(stdout), Some("203.0.113.42".to_string()));
    }

    #[test]
    fn test_extract_server_ip_public_ip() {
        let stdout = "public_ip = 10.0.0.5\n";
        assert_eq!(extract_server_ip(stdout), Some("10.0.0.5".to_string()));
    }

    #[test]
    fn test_extract_server_ip_none() {
        assert_eq!(extract_server_ip("no ip here"), None);
    }

    #[test]
    fn test_strategy_for_factory() {
        // Verify the factory returns something for each variant (no panic).
        let _ = strategy_for(&DeployTarget::Local);
        let _ = strategy_for(&DeployTarget::Cloud);
        let _ = strategy_for(&DeployTarget::Server);
    }

    #[test]
    fn test_install_action_as_str() {
        assert_eq!(InstallAction::Plan.as_str(), "plan");
        assert_eq!(InstallAction::Apply.as_str(), "apply");
        assert_eq!(InstallAction::Destroy.as_str(), "destroy");
    }

    #[test]
    fn test_command_output_success() {
        let output = CommandOutput {
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
        };
        assert!(output.success());

        let output = CommandOutput {
            exit_code: 1,
            stdout: String::new(),
            stderr: "fail".to_string(),
        };
        assert!(!output.success());
    }

    #[test]
    fn test_install_command_remove_after_default() {
        let cmd = InstallContainerCommand::new(None);
        let args = cmd.build_args();
        assert!(args.contains(&"--rm".to_string()));
    }

    #[test]
    fn test_install_command_no_remove() {
        let cmd = InstallContainerCommand::new(None).remove_after(false);
        let args = cmd.build_args();
        assert!(!args.contains(&"--rm".to_string()));
    }
}
