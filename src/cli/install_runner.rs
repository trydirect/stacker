use std::path::{Path, PathBuf};

use crate::cli::config_parser::{DeployTarget, StackerConfig};
use crate::cli::error::CliError;

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
    use crate::cli::config_parser::{CloudConfig, CloudProvider, ConfigBuilder, ServerConfig};

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
                region: Some("fsn1".to_string()),
                size: Some("cx21".to_string()),
                ssh_key: Some(PathBuf::from("/home/user/.ssh/id_ed25519")),
            })
            .build()
            .unwrap()
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
