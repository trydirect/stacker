use std::convert::TryFrom;
use std::path::{Path, PathBuf};

use crate::cli::config_parser::{AppType, DeployTarget, StackerConfig};
use crate::cli::credentials::{CredentialsManager, FileCredentialStore};
use crate::cli::error::CliError;
use crate::cli::generator::compose::ComposeDefinition;
use crate::cli::generator::dockerfile::DockerfileBuilder;
use crate::cli::install_runner::{
    strategy_for, CommandExecutor, DeployContext, DeployResult, ShellExecutor,
};
use crate::console::commands::CallableTrait;

/// Default config filename.
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Output directory for generated artifacts.
const OUTPUT_DIR: &str = ".stacker";

/// `stacker deploy [--target local|cloud|server] [--file stacker.yml] [--dry-run] [--force-rebuild]`
///
/// Generates Dockerfile + docker-compose from stacker.yml, then
/// deploys using the appropriate strategy (local, cloud, or server).
pub struct DeployCommand {
    pub target: Option<String>,
    pub file: Option<String>,
    pub dry_run: bool,
    pub force_rebuild: bool,
}

impl DeployCommand {
    pub fn new(
        target: Option<String>,
        file: Option<String>,
        dry_run: bool,
        force_rebuild: bool,
    ) -> Self {
        Self {
            target,
            file,
            dry_run,
            force_rebuild,
        }
    }
}

/// Parse a deploy target string into `DeployTarget`.
fn parse_deploy_target(s: &str) -> Result<DeployTarget, CliError> {
    let json = format!("\"{}\"", s.to_lowercase());
    serde_json::from_str::<DeployTarget>(&json).map_err(|_| {
        CliError::ConfigValidation(format!(
            "Unknown deploy target '{}'. Valid targets: local, cloud, server",
            s
        ))
    })
}

/// Core deploy logic, extracted for testability.
///
/// Takes injectable `CommandExecutor` so tests can mock shell calls.
pub fn run_deploy(
    project_dir: &Path,
    config_file: Option<&str>,
    target_override: Option<&str>,
    dry_run: bool,
    force_rebuild: bool,
    executor: &dyn CommandExecutor,
) -> Result<DeployResult, CliError> {
    // 1. Load config
    let config_path = match config_file {
        Some(f) => project_dir.join(f),
        None => project_dir.join(DEFAULT_CONFIG_FILE),
    };

    let config = StackerConfig::from_file(&config_path)?;

    // 2. Resolve deploy target (flag > config)
    let deploy_target = match target_override {
        Some(t) => parse_deploy_target(t)?,
        None => config.deploy.target,
    };

    // 3. Cloud/server prerequisites
    if deploy_target == DeployTarget::Cloud {
        // Verify login
        let cred_manager = CredentialsManager::with_default_store();
        cred_manager.require_valid_token("cloud deploy")?;
    }

    // 4. Validate via strategy
    let strategy = strategy_for(&deploy_target);
    strategy.validate(&config)?;

    // 5. Generate artifacts into .stacker/
    let output_dir = project_dir.join(OUTPUT_DIR);
    std::fs::create_dir_all(&output_dir)?;

    // 5a. Dockerfile
    let needs_dockerfile = config.app.image.is_none() && config.app.dockerfile.is_none();
    let dockerfile_path = output_dir.join("Dockerfile");

    if needs_dockerfile {
        let builder = DockerfileBuilder::from(config.app.app_type);
        builder.write_to(&dockerfile_path, force_rebuild)?;
    }

    // 5b. docker-compose.yml
    let compose_path = if let Some(ref existing) = config.deploy.compose_file {
        project_dir.join(existing)
    } else {
        let compose_out = output_dir.join("docker-compose.yml");
        let compose = ComposeDefinition::try_from(&config)?;
        compose.write_to(&compose_out, force_rebuild)?;
        compose_out
    };

    // 5c. Report hooks (dry-run)
    if dry_run {
        if let Some(ref pre_build) = config.hooks.pre_build {
            eprintln!("  Hook (pre_build): {}", pre_build.display());
        }
    }

    // 6. Deploy
    let context = DeployContext {
        config_path: config_path.clone(),
        compose_path: compose_path.clone(),
        project_dir: project_dir.to_path_buf(),
        dry_run,
        image: None,
    };

    let result = strategy.deploy(&config, &context, executor)?;

    Ok(result)
}

impl CallableTrait for DeployCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let executor = ShellExecutor;

        let result = run_deploy(
            &project_dir,
            self.file.as_deref(),
            self.target.as_deref(),
            self.dry_run,
            self.force_rebuild,
            &executor,
        )?;

        eprintln!("✓ {}", result.message);
        if let Some(ip) = &result.server_ip {
            eprintln!("  Server IP: {}", ip);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::install_runner::CommandOutput;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Mock executor that records commands and returns configurable output.
    struct MockExecutor {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        output: CommandOutput,
    }

    impl MockExecutor {
        fn success() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                output: CommandOutput {
                    exit_code: 0,
                    stdout: "ok".to_string(),
                    stderr: String::new(),
                },
            }
        }

        fn recorded_calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, program: &str, args: &[&str]) -> Result<CommandOutput, CliError> {
            self.calls.lock().unwrap().push((
                program.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
            Ok(self.output.clone())
        }
    }

    /// Create a tempdir with a minimal stacker.yml for local deploy.
    fn setup_local_project(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }
        dir
    }

    fn minimal_config_yaml() -> String {
        "name: test-app\napp:\n  type: static\n  path: .\n".to_string()
    }

    fn cloud_config_yaml() -> String {
        "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  target: cloud\n  cloud:\n    provider: hetzner\n    region: eu-central\n    size: cx11\n".to_string()
    }

    fn server_config_yaml() -> String {
        "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  target: server\n  server:\n    host: 1.2.3.4\n    user: root\n    port: 22\n".to_string()
    }

    // ── Tests ────────────────────────────────────────

    #[test]
    fn test_deploy_local_dry_run_generates_files() {
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("stacker.yml", &minimal_config_yaml()),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());

        // Generated files should exist
        assert!(dir.path().join(".stacker/Dockerfile").exists());
        assert!(dir.path().join(".stacker/docker-compose.yml").exists());
    }

    #[test]
    fn test_deploy_local_preserves_existing_dockerfile() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\n  dockerfile: Dockerfile\n";
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("Dockerfile", "FROM custom:latest\nCOPY . /custom"),
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());

        // Custom Dockerfile should not be overwritten
        let df = std::fs::read_to_string(dir.path().join("Dockerfile")).unwrap();
        assert!(df.contains("custom:latest"));

        // .stacker/Dockerfile should NOT be generated (app.dockerfile is set)
        assert!(!dir.path().join(".stacker/Dockerfile").exists());
    }

    #[test]
    fn test_deploy_local_uses_existing_compose() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  compose_file: docker-compose.yml\n";
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("docker-compose.yml", "version: '3.8'\nservices:\n  web:\n    image: nginx\n"),
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());

        // .stacker/docker-compose.yml should NOT be generated
        assert!(!dir.path().join(".stacker/docker-compose.yml").exists());
    }

    #[test]
    fn test_deploy_local_with_image_skips_build() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\n  image: nginx:latest\n";
        let dir = setup_local_project(&[
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());

        // No Dockerfile should be generated (using image)
        assert!(!dir.path().join(".stacker/Dockerfile").exists());
    }

    #[test]
    fn test_deploy_cloud_requires_login() {
        let dir = setup_local_project(&[
            ("stacker.yml", &cloud_config_yaml()),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, None, true, false, &executor);
        assert!(result.is_err());

        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Login required") || err.contains("login"),
            "Expected login error, got: {}",
            err
        );
    }

    #[test]
    fn test_deploy_cloud_requires_provider() {
        // Cloud target but no cloud config
        let config = "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  target: cloud\n";
        let dir = setup_local_project(&[
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        // This should fail at validation since no credentials exist
        let result = run_deploy(dir.path(), None, None, true, false, &executor);
        assert!(result.is_err());
    }

    #[test]
    fn test_deploy_server_requires_host() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  target: server\n";
        let dir = setup_local_project(&[
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, None, true, false, &executor);
        assert!(result.is_err());

        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("host") || err.contains("Host") || err.contains("server"),
            "Expected server host error, got: {}", err);
    }

    #[test]
    fn test_deploy_missing_config_file() {
        let dir = TempDir::new().unwrap();
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, None, true, false, &executor);
        assert!(result.is_err());

        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("not found") || err.contains("Configuration"),
            "Expected config not found error, got: {}", err);
    }

    #[test]
    fn test_deploy_custom_file_flag() {
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("custom.yml", &minimal_config_yaml()),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), Some("custom.yml"), Some("local"), true, false, &executor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_deploy_force_rebuild() {
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("stacker.yml", &minimal_config_yaml()),
        ]);
        let executor = MockExecutor::success();

        // First deploy creates files
        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());

        // Second deploy without force_rebuild should fail (files exist)
        let result2 = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result2.is_err());

        // With force_rebuild should succeed
        let result3 = run_deploy(dir.path(), None, Some("local"), true, true, &executor);
        assert!(result3.is_ok());
    }

    #[test]
    fn test_deploy_target_strategy_dispatch() {
        // Validate that strategy_for returns the right type
        let local = strategy_for(&DeployTarget::Local);
        let cloud = strategy_for(&DeployTarget::Cloud);
        let server = strategy_for(&DeployTarget::Server);

        // We can't check concrete types directly, but we can ensure
        // validation behavior matches expectations:
        let minimal_config = StackerConfig::from_str("name: test\napp:\n  type: static\n").unwrap();

        // Local always passes validation
        assert!(local.validate(&minimal_config).is_ok());
        // Cloud fails without cloud config
        assert!(cloud.validate(&minimal_config).is_err());
        // Server fails without server config
        assert!(server.validate(&minimal_config).is_err());
    }

    #[test]
    fn test_deploy_runs_pre_build_hook_noted() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\nhooks:\n  pre_build: ./build.sh\n";
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        // Dry-run should succeed (hooks are just noted, not executed in dry-run)
        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_deploy_target_valid() {
        assert_eq!(parse_deploy_target("local").unwrap(), DeployTarget::Local);
        assert_eq!(parse_deploy_target("cloud").unwrap(), DeployTarget::Cloud);
        assert_eq!(parse_deploy_target("server").unwrap(), DeployTarget::Server);
        assert_eq!(parse_deploy_target("LOCAL").unwrap(), DeployTarget::Local);
    }

    #[test]
    fn test_parse_deploy_target_invalid() {
        let result = parse_deploy_target("kubernetes");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Unknown deploy target"));
    }
}
