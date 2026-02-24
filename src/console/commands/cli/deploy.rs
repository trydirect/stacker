use std::convert::TryFrom;
use std::path::{Path, PathBuf};

use crate::cli::ai_client::{
    build_prompt, create_provider, ollama_complete_streaming, AiTask, PromptContext,
};
use crate::cli::config_parser::{AiProviderType, AppType, DeployTarget, StackerConfig};
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

fn parse_ai_provider(s: &str) -> Result<AiProviderType, CliError> {
    let json = format!("\"{}\"", s.trim().to_lowercase());
    serde_json::from_str::<AiProviderType>(&json).map_err(|_| {
        CliError::ConfigValidation(
            "Unknown AI provider. Use: openai, anthropic, ollama, custom".to_string(),
        )
    })
}

fn resolve_ai_from_env_or_config(project_dir: &Path, config_file: Option<&str>) -> Result<crate::cli::config_parser::AiConfig, CliError> {
    let config_path = match config_file {
        Some(f) => project_dir.join(f),
        None => project_dir.join(DEFAULT_CONFIG_FILE),
    };

    let mut ai = if config_path.exists() {
        StackerConfig::from_file(&config_path)?.ai
    } else {
        Default::default()
    };

    if let Ok(provider) = std::env::var("STACKER_AI_PROVIDER") {
        ai.provider = parse_ai_provider(&provider)?;
        ai.enabled = true;
    }

    if let Ok(model) = std::env::var("STACKER_AI_MODEL") {
        if !model.trim().is_empty() {
            ai.model = Some(model);
            ai.enabled = true;
        }
    }

    if let Ok(endpoint) = std::env::var("STACKER_AI_ENDPOINT") {
        if !endpoint.trim().is_empty() {
            ai.endpoint = Some(endpoint);
            ai.enabled = true;
        }
    }

    if let Ok(timeout) = std::env::var("STACKER_AI_TIMEOUT") {
        if let Ok(value) = timeout.parse::<u64>() {
            ai.timeout = value;
            ai.enabled = true;
        }
    }

    if let Ok(generic_key) = std::env::var("STACKER_AI_API_KEY") {
        if !generic_key.trim().is_empty() {
            ai.api_key = Some(generic_key);
            ai.enabled = true;
        }
    }

    if ai.api_key.is_none() {
        match ai.provider {
            AiProviderType::Openai => {
                if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                    if !key.trim().is_empty() {
                        ai.api_key = Some(key);
                        ai.enabled = true;
                    }
                }
            }
            AiProviderType::Anthropic => {
                if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                    if !key.trim().is_empty() {
                        ai.api_key = Some(key);
                        ai.enabled = true;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ai)
}

fn fallback_troubleshooting_hints(reason: &str) -> Vec<String> {
    let lower = reason.to_lowercase();
    let mut hints = Vec::new();

    if lower.contains("npm ci") {
        hints.push("npm ci failed: ensure package-lock.json exists and is in sync with package.json".to_string());
        hints.push("Try locally: npm ci --production (or npm ci) to see the full dependency error".to_string());
    }
    if lower.contains("the attribute `version` is obsolete") || lower.contains("attribute `version` is obsolete") {
        hints.push("docker-compose version warning: remove top-level 'version:' from .stacker/docker-compose.yml".to_string());
    }
    if lower.contains("failed to solve") {
        hints.push("Docker build step failed: inspect the failing Dockerfile line and run docker build manually for verbose output".to_string());
    }
    if lower.contains("permission denied") || lower.contains("eacces") {
        hints.push("Permission issue detected: verify file ownership and executable bits for scripts copied into the image".to_string());
    }
    if lower.contains("no such file") || lower.contains("not found") {
        hints.push("Missing file in build context: confirm COPY paths and .dockerignore rules".to_string());
    }
    if lower.contains("network") || lower.contains("timed out") {
        hints.push("Network/timeout issue: retry build and verify registry connectivity".to_string());
    }
    if lower.contains("port is already allocated")
        || lower.contains("bind for 0.0.0.0")
        || lower.contains("failed programming external connectivity")
    {
        hints.push("Port conflict: another process/container already uses this host port (for example 3000).".to_string());
        hints.push("Find the owner with: lsof -nP -iTCP:3000 -sTCP:LISTEN".to_string());
        hints.push("Then stop it (docker compose down / docker rm -f <container>) or change ports in stacker.yml".to_string());
    }
    if lower.contains("remote orchestrator request failed")
        && lower.contains("http error")
        && lower.contains("404")
        && (lower.contains("<!doctype html") || lower.contains("<html"))
    {
        hints.push("Remote orchestrator URL looks incorrect (received frontend 404 HTML instead of User Service JSON).".to_string());
        hints.push("If you logged in with /server/user/auth/login, deploy expects User Service base URL ending with /server/user.".to_string());
        hints.push("Try re-login with: stacker-cli login --auth-url https://dev.try.direct/server/user/auth/login".to_string());
    }
    if lower.contains("orphan containers") {
        hints.push("Orphan containers detected: run docker compose -f .stacker/docker-compose.yml down --remove-orphans".to_string());
    }
    if lower.contains("manifest unknown") || lower.contains("pull access denied") {
        hints.push("Image pull failed: the configured image tag is not available in the registry".to_string());
        if let Some(image) = extract_missing_image(reason) {
            hints.push(format!("Missing image detected: {}", image));
            hints.push(format!("Build and tag locally: docker build -t {} .", image));
            hints.push(format!("If using a remote registry, push it first: docker push {}", image));
        } else {
            hints.push("Build locally first (docker build -t <image:tag> .) or use an existing published tag".to_string());
        }
        hints.push("Alternative: remove app.image in stacker.yml so Stacker generates/uses a local build context".to_string());
    }

    if hints.is_empty() {
        hints.push("Run docker compose -f .stacker/docker-compose.yml build --no-cache for detailed build logs".to_string());
        hints.push("Inspect .stacker/Dockerfile and .stacker/docker-compose.yml for invalid paths and commands".to_string());
        hints.push("If the issue is dependency-related, run the failing install command locally first".to_string());
    }

    hints
}

fn extract_missing_image(reason: &str) -> Option<String> {
    for marker in ["manifest for ", "pull access denied for "] {
        if let Some(start) = reason.find(marker) {
            let image_start = start + marker.len();
            let tail = &reason[image_start..];
            let image = tail
                .split(|c: char| c.is_whitespace() || c == ',' || c == '\n')
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .to_string();
            if !image.is_empty() {
                return Some(image);
            }
        }
    }
    None
}

fn ensure_env_file_if_needed(config: &StackerConfig, project_dir: &Path) -> Result<(), CliError> {
    let env_file = match &config.env_file {
        Some(path) => path,
        None => return Ok(()),
    };

    let env_path = if env_file.is_absolute() {
        env_file.clone()
    } else {
        project_dir.join(env_file)
    };

    if env_path.exists() {
        return Ok(());
    }

    if let Some(parent) = env_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut content = String::from("# Auto-created by Stacker because env_file was configured\n");
    if !config.env.is_empty() {
        let mut keys: Vec<&String> = config.env.keys().collect();
        keys.sort();
        for key in keys {
            content.push_str(&format!("{}={}\n", key, config.env[key]));
        }
    }

    std::fs::write(&env_path, content)?;
    eprintln!("  Created missing env file: {}", env_path.display());
    Ok(())
}

fn normalize_generated_compose_paths(compose_path: &Path) -> Result<(), CliError> {
    let is_stacker_compose = compose_path
        .components()
        .any(|c| c.as_os_str() == OUTPUT_DIR);

    if !is_stacker_compose || !compose_path.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(compose_path)?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&raw)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to parse compose file: {e}")))?;

    let mut changed = false;

    if let serde_yaml::Value::Mapping(ref mut root) = doc {
        // Remove obsolete compose version key.
        if root.remove(serde_yaml::Value::String("version".to_string())).is_some() {
            changed = true;
        }

        let services_key = serde_yaml::Value::String("services".to_string());
        if let Some(serde_yaml::Value::Mapping(services)) = root.get_mut(&services_key) {
            for (service_key, service_value) in services.iter_mut() {
                let service_name = service_key.as_str().unwrap_or("");
                let service_map = match service_value {
                    serde_yaml::Value::Mapping(m) => m,
                    _ => continue,
                };

                let build_key = serde_yaml::Value::String("build".to_string());
                let build_val = match service_map.get_mut(&build_key) {
                    Some(v) => v,
                    None => continue,
                };

                let build_map = match build_val {
                    serde_yaml::Value::Mapping(m) => m,
                    _ => continue,
                };

                let context_key = serde_yaml::Value::String("context".to_string());
                let dockerfile_key = serde_yaml::Value::String("dockerfile".to_string());

                let current_context = build_map
                    .get(&context_key)
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string();

                let dockerfile = build_map
                    .get(&dockerfile_key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let dockerfile_points_to_stacker = dockerfile
                    .as_deref()
                    .map(|d| d.starts_with(".stacker/"))
                    .unwrap_or(false);

                if dockerfile_points_to_stacker && (current_context == "." || current_context == "./") {
                    build_map.insert(
                        context_key.clone(),
                        serde_yaml::Value::String("..".to_string()),
                    );
                    changed = true;
                }

                if service_name == "app" && (current_context == "." || current_context == "./") {
                    build_map.insert(
                        context_key,
                        serde_yaml::Value::String("..".to_string()),
                    );
                    changed = true;
                }
            }
        }
    }

    if changed {
        let updated = serde_yaml::to_string(&doc)
            .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize compose file: {e}")))?;
        std::fs::write(compose_path, updated)?;
        eprintln!("  Normalized {}/docker-compose.yml paths", OUTPUT_DIR);
    }

    Ok(())
}

fn compose_app_build_source(compose_path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(compose_path).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw).ok()?;

    let root = match doc {
        serde_yaml::Value::Mapping(m) => m,
        _ => return None,
    };

    let services_key = serde_yaml::Value::String("services".to_string());
    let app_key = serde_yaml::Value::String("app".to_string());
    let build_key = serde_yaml::Value::String("build".to_string());
    let context_key = serde_yaml::Value::String("context".to_string());
    let dockerfile_key = serde_yaml::Value::String("dockerfile".to_string());

    let services = match root.get(&services_key) {
        Some(serde_yaml::Value::Mapping(m)) => m,
        _ => return None,
    };
    let app = match services.get(&app_key) {
        Some(serde_yaml::Value::Mapping(m)) => m,
        _ => return None,
    };
    let build = app.get(&build_key)?;

    let compose_dir = compose_path.parent().unwrap_or_else(|| Path::new("."));

    match build {
        serde_yaml::Value::String(context_str) => {
            let context_path = PathBuf::from(context_str);
            let context_abs = if context_path.is_absolute() {
                context_path
            } else {
                compose_dir.join(context_path)
            };
            let dockerfile_abs = context_abs.join("Dockerfile");
            Some(format!(
                "context={}, dockerfile={}",
                context_abs.display(),
                dockerfile_abs.display()
            ))
        }
        serde_yaml::Value::Mapping(build_map) => {
            let context_raw = build_map
                .get(&context_key)
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let dockerfile_raw = build_map
                .get(&dockerfile_key)
                .and_then(|v| v.as_str())
                .unwrap_or("Dockerfile");

            let context_path = PathBuf::from(context_raw);
            let context_abs = if context_path.is_absolute() {
                context_path
            } else {
                compose_dir.join(context_path)
            };

            let dockerfile_path = PathBuf::from(dockerfile_raw);
            let dockerfile_abs = if dockerfile_path.is_absolute() {
                dockerfile_path
            } else {
                context_abs.join(dockerfile_path)
            };

            Some(format!(
                "context={}, dockerfile={}",
                context_abs.display(),
                dockerfile_abs.display()
            ))
        }
        _ => None,
    }
}

fn build_troubleshoot_error_log(project_dir: &Path, reason: &str) -> String {
    let dockerfile_path = project_dir.join(OUTPUT_DIR).join("Dockerfile");
    let compose_path = project_dir.join(OUTPUT_DIR).join("docker-compose.yml");

    let dockerfile = std::fs::read_to_string(&dockerfile_path).unwrap_or_default();
    let compose = std::fs::read_to_string(&compose_path).unwrap_or_default();

    let dockerfile_snippet = if dockerfile.is_empty() {
        "(not found)".to_string()
    } else {
        dockerfile.chars().take(4000).collect()
    };

    let compose_snippet = if compose.is_empty() {
        "(not found)".to_string()
    } else {
        compose.chars().take(4000).collect()
    };

    format!(
        "Deploy error:\n{}\n\nGenerated Dockerfile (.stacker/Dockerfile):\n{}\n\nGenerated Compose (.stacker/docker-compose.yml):\n{}",
        reason, dockerfile_snippet, compose_snippet
    )
}

fn print_ai_deploy_help(project_dir: &Path, config_file: Option<&str>, err: &CliError) {
    let reason = match err {
        CliError::DeployFailed { reason, .. } => reason,
        _ => return,
    };

    eprintln!("\nTroubleshooting help:");

    let ai_config = match resolve_ai_from_env_or_config(project_dir, config_file) {
        Ok(cfg) => cfg,
        Err(load_err) => {
            eprintln!("  Could not load AI config for troubleshooting: {}", load_err);
            for hint in fallback_troubleshooting_hints(reason) {
                eprintln!("  - {}", hint);
            }
            eprintln!("  Tip: enable AI with stacker init --with-ai or set STACKER_AI_PROVIDER=ollama");
            return;
        }
    };

    if !ai_config.enabled {
        eprintln!("  AI troubleshooting disabled (ai.enabled=false).");
        for hint in fallback_troubleshooting_hints(reason) {
            eprintln!("  - {}", hint);
        }
        eprintln!("  Tip: enable AI in stacker.yml if you want AI troubleshooting suggestions");
        return;
    }

    let error_log = build_troubleshoot_error_log(project_dir, reason);
    let ctx = PromptContext {
        project_type: None,
        files: vec![".stacker/Dockerfile".to_string(), ".stacker/docker-compose.yml".to_string()],
        error_log: Some(error_log),
        current_config: None,
    };
    let (system, prompt) = build_prompt(AiTask::Troubleshoot, &ctx);

    if ai_config.provider == AiProviderType::Ollama {
        eprintln!("  AI suggestion (streaming from Ollama):");
        match ollama_complete_streaming(&ai_config, &prompt, &system) {
            Ok(answer) => {
                if answer.trim().is_empty() {
                    eprintln!("    (empty AI response)");
                }
                eprintln!();
            }
            Err(ai_err) => {
                eprintln!("  AI troubleshooting unavailable: {}", ai_err);
                for hint in fallback_troubleshooting_hints(reason) {
                    eprintln!("  - {}", hint);
                }
                eprintln!("  Tip: set STACKER_AI_PROVIDER=ollama and ensure Ollama is running");
            }
        }
        return;
    }

    eprintln!("  AI request in progress...");
    match create_provider(&ai_config).and_then(|provider| provider.complete(&prompt, &system)) {
        Ok(answer) => {
            eprintln!("  AI suggestion:");
            for line in answer.lines().take(20) {
                eprintln!("    {}", line);
            }
        }
        Err(ai_err) => {
            eprintln!("  AI troubleshooting unavailable: {}", ai_err);
            for hint in fallback_troubleshooting_hints(reason) {
                eprintln!("  - {}", hint);
            }
            eprintln!("  Tip: set STACKER_AI_PROVIDER=ollama and ensure Ollama is running");
        }
    }
}

/// `stacker deploy [--target local|cloud|server] [--file stacker.yml] [--dry-run] [--force-rebuild]`
/// `stacker deploy --project=myapp --target cloud --key devops --server bastion`
///
/// Generates Dockerfile + docker-compose from stacker.yml, then
/// deploys using the appropriate strategy (local, cloud, or server).
///
/// For remote cloud deploys, the CLI now goes through the Stacker server API
/// instead of calling User Service directly:
///   1. Resolves (or auto-creates) the project on the Stacker server
///   2. Looks up saved cloud credentials by provider (or passes env-var creds)
///   3. Looks up saved server by name (optional)
///   4. Calls `POST /project/{id}/deploy[/{cloud_id}]`
pub struct DeployCommand {
    pub target: Option<String>,
    pub file: Option<String>,
    pub dry_run: bool,
    pub force_rebuild: bool,
    /// Override project name (--project flag)
    pub project_name: Option<String>,
    /// Override cloud key name (--key flag)
    pub key_name: Option<String>,
    /// Override server name (--server flag)
    pub server_name: Option<String>,
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
            project_name: None,
            key_name: None,
            server_name: None,
        }
    }

    /// Builder method to set remote override flags from CLI args.
    pub fn with_remote_overrides(
        mut self,
        project: Option<String>,
        key: Option<String>,
        server: Option<String>,
    ) -> Self {
        self.project_name = project;
        self.key_name = key;
        self.server_name = server;
        self
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

/// Override values from CLI flags for remote cloud deploys.
#[derive(Debug, Clone, Default)]
pub struct RemoteDeployOverrides {
    pub project_name: Option<String>,
    pub key_name: Option<String>,
    pub server_name: Option<String>,
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
    remote_overrides: &RemoteDeployOverrides,
) -> Result<DeployResult, CliError> {
    // 1. Load config
    let config_path = match config_file {
        Some(f) => project_dir.join(f),
        None => project_dir.join(DEFAULT_CONFIG_FILE),
    };

    let config = StackerConfig::from_file(&config_path)?;
    ensure_env_file_if_needed(&config, project_dir)?;

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
        if force_rebuild || !dockerfile_path.exists() {
            let builder = DockerfileBuilder::from(config.app.app_type);
            builder.write_to(&dockerfile_path, force_rebuild)?;
        } else {
            eprintln!("  Using existing {}/Dockerfile (use --force-rebuild to regenerate)", OUTPUT_DIR);
        }
    }

    // 5b. docker-compose.yml
    let compose_path = if let Some(ref existing) = config.deploy.compose_file {
        let configured_path = project_dir.join(existing);
        if configured_path.exists() {
            configured_path
        } else {
            let generated_fallback = output_dir.join("docker-compose.yml");
            if generated_fallback.exists() {
                eprintln!(
                    "  Configured compose file not found: {}. Falling back to {}",
                    configured_path.display(),
                    generated_fallback.display()
                );
                generated_fallback
            } else {
                return Err(CliError::ConfigValidation(format!(
                    "Compose file not found: {}",
                    configured_path.display()
                )));
            }
        }
    } else {
        let compose_out = output_dir.join("docker-compose.yml");
        if force_rebuild || !compose_out.exists() {
            let compose = ComposeDefinition::try_from(&config)?;
            compose.write_to(&compose_out, force_rebuild)?;
        } else {
            eprintln!("  Using existing {}/docker-compose.yml (use --force-rebuild to regenerate)", OUTPUT_DIR);
        }
        compose_out
    };

    normalize_generated_compose_paths(&compose_path)?;

    // 5b.1 Surface build source paths to avoid confusion.
    if let Some(image) = &config.app.image {
        eprintln!("  App image source: image={} (no local Dockerfile build)", image);
    } else if let Some(build_src) = compose_app_build_source(&compose_path) {
        eprintln!("  App build source: {}", build_src);
    } else if let Some(dockerfile) = &config.app.dockerfile {
        let dockerfile_display = if dockerfile.is_absolute() {
            dockerfile.display().to_string()
        } else {
            project_dir.join(dockerfile).display().to_string()
        };
        eprintln!("  App build source: Dockerfile={}", dockerfile_display);
    } else {
        eprintln!("  App build source: Dockerfile={}", dockerfile_path.display());
    }
    eprintln!("  Compose file: {}", compose_path.display());

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
        image: config
            .deploy
            .cloud
            .as_ref()
            .and_then(|cloud| cloud.install_image.clone()),
        project_name_override: remote_overrides.project_name.clone(),
        key_name_override: remote_overrides.key_name.clone(),
        server_name_override: remote_overrides.server_name.clone(),
    };

    let result = strategy.deploy(&config, &context, executor)?;

    Ok(result)
}

impl CallableTrait for DeployCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = std::env::current_dir()?;
        let executor = ShellExecutor;

        // Build remote overrides from CLI flags
        let remote_overrides = RemoteDeployOverrides {
            project_name: self.project_name.clone(),
            key_name: self.key_name.clone(),
            server_name: self.server_name.clone(),
        };

        let result = run_deploy(
            &project_dir,
            self.file.as_deref(),
            self.target.as_deref(),
            self.dry_run,
            self.force_rebuild,
            &executor,
            &remote_overrides,
        );

        let result = match result {
            Ok(result) => result,
            Err(err) => {
                if let CliError::LoginRequired { .. } = &err {
                    eprintln!("\nHint: run `stacker login` and retry deploy.");
                }
                print_ai_deploy_help(&project_dir, self.file.as_deref(), &err);
                return Err(Box::new(err));
            }
        };

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

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
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

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
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

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_ok());

        // .stacker/docker-compose.yml should NOT be generated
        assert!(!dir.path().join(".stacker/docker-compose.yml").exists());
    }

    #[test]
    fn test_deploy_falls_back_when_configured_compose_missing() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  compose_file: stacker/docker-compose.yml\n";
        let dir = setup_local_project(&[
            ("index.html", "<h1>hello</h1>"),
            ("stacker.yml", config),
            (".stacker/docker-compose.yml", "services:\n  app:\n    image: nginx\n"),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_ok());
    }

    #[test]
    fn test_deploy_local_with_image_skips_build() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\n  image: nginx:latest\n";
        let dir = setup_local_project(&[
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
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

        let result = run_deploy(dir.path(), None, None, true, false, &executor, &RemoteDeployOverrides::default());
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
        let result = run_deploy(dir.path(), None, None, true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_deploy_server_requires_host() {
        let config = "name: test-app\napp:\n  type: static\n  path: .\ndeploy:\n  target: server\n";
        let dir = setup_local_project(&[
            ("stacker.yml", config),
        ]);
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, None, true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_err());

        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("host") || err.contains("Host") || err.contains("server"),
            "Expected server host error, got: {}", err);
    }

    #[test]
    fn test_deploy_missing_config_file() {
        let dir = TempDir::new().unwrap();
        let executor = MockExecutor::success();

        let result = run_deploy(dir.path(), None, None, true, false, &executor, &RemoteDeployOverrides::default());
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

        let result = run_deploy(dir.path(), Some("custom.yml"), Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
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
        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_ok());

        // Second deploy without force_rebuild should succeed (reuses existing files)
        let result2 = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result2.is_ok());

        // With force_rebuild should also succeed (regenerates files)
        let result3 = run_deploy(dir.path(), None, Some("local"), true, true, &executor, &RemoteDeployOverrides::default());
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
        let result = run_deploy(dir.path(), None, Some("local"), true, false, &executor, &RemoteDeployOverrides::default());
        assert!(result.is_ok());
    }

    #[test]
    fn test_fallback_hints_for_npm_ci_error() {
        let hints = fallback_troubleshooting_hints("failed to solve: /bin/sh -c npm ci --production");
        assert!(hints.iter().any(|h| h.contains("npm ci failed")));
    }

    #[test]
    fn test_compose_app_build_source_reads_context_and_dockerfile() {
        let dir = TempDir::new().unwrap();
        let compose_path = dir.path().join(".stacker").join("docker-compose.yml");
        std::fs::create_dir_all(compose_path.parent().unwrap()).unwrap();
        std::fs::write(
            &compose_path,
            "services:\n  app:\n    build:\n      context: ..\n      dockerfile: .stacker/Dockerfile\n",
        )
        .unwrap();

        let source = compose_app_build_source(&compose_path).unwrap();
        assert!(source.contains("context="));
        assert!(source.contains("dockerfile="));
        assert!(source.contains(".stacker/Dockerfile"));
    }

    #[test]
    fn test_build_troubleshoot_error_log_handles_missing_files() {
        let dir = TempDir::new().unwrap();
        let log = build_troubleshoot_error_log(dir.path(), "docker compose failed");
        assert!(log.contains("docker compose failed"));
        assert!(log.contains("(not found)"));
    }

        #[test]
        fn test_normalize_generated_compose_paths_fixes_stacker_context_and_version() {
                let dir = TempDir::new().unwrap();
                let stacker_dir = dir.path().join(".stacker");
                std::fs::create_dir_all(&stacker_dir).unwrap();

                let compose_path = stacker_dir.join("docker-compose.yml");
                let compose = r#"
version: "3.9"
services:
    app:
        build:
            context: .
            dockerfile: .stacker/Dockerfile
"#;
                std::fs::write(&compose_path, compose).unwrap();

                normalize_generated_compose_paths(&compose_path).unwrap();

                let normalized = std::fs::read_to_string(&compose_path).unwrap();
                assert!(!normalized.contains("version:"));
                assert!(normalized.contains("context: .."));
                assert!(normalized.contains("dockerfile: .stacker/Dockerfile"));
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

    #[test]
    fn test_extract_missing_image_from_manifest_error() {
        let reason = "manifest for optimum/optimumcode:latest not found: manifest unknown";
        let image = extract_missing_image(reason);
        assert_eq!(image.as_deref(), Some("optimum/optimumcode:latest"));
    }

    #[test]
    fn test_fallback_hints_for_manifest_unknown() {
        let hints = fallback_troubleshooting_hints(
            "docker compose failed: manifest for optimum/optimumcode:latest not found: manifest unknown"
        );
        assert!(hints.iter().any(|h| h.contains("Image pull failed")));
        assert!(hints.iter().any(|h| h.contains("docker build -t optimum/optimumcode:latest .")));
    }

    #[test]
    fn test_fallback_hints_for_port_conflict() {
        let hints = fallback_troubleshooting_hints(
            "failed to set up container networking: driver failed programming external connectivity on endpoint app: Bind for 0.0.0.0:3000 failed: port is already allocated"
        );
        assert!(hints.iter().any(|h| h.contains("Port conflict")));
        assert!(hints.iter().any(|h| h.contains("lsof -nP -iTCP:3000")));
    }

    #[test]
    fn test_fallback_hints_for_orphan_containers() {
        let hints = fallback_troubleshooting_hints(
            "Found orphan containers ([stackerdb]) for this project"
        );
        assert!(hints.iter().any(|h| h.contains("--remove-orphans")));
    }

    #[test]
    fn test_fallback_hints_for_remote_orchestrator_html_404() {
        let hints = fallback_troubleshooting_hints(
            "Remote orchestrator request failed: HTTP error: User Service error (404): <!DOCTYPE html><html><head><title>Page not found</title></head>"
        );
        assert!(hints.iter().any(|h| h.contains("URL looks incorrect")));
        assert!(hints.iter().any(|h| h.contains("/server/user/auth/login")));
    }

    #[test]
    fn test_ensure_env_file_is_created_when_missing() {
        let dir = TempDir::new().unwrap();
        let config = StackerConfig::from_str(
            "name: env-app\napp:\n  type: static\nenv_file: .env\nenv:\n  APP_ENV: production\n"
        )
        .unwrap();

        ensure_env_file_if_needed(&config, dir.path()).unwrap();

        let env_path = dir.path().join(".env");
        assert!(env_path.exists());
        let content = std::fs::read_to_string(env_path).unwrap();
        assert!(content.contains("APP_ENV=production"));
    }
}
