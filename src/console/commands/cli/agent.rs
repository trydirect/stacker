//! `stacker agent` — CLI subcommands for Status Panel agent control.
//!
//! Every command follows the pull-only architecture:
//!
//! ```text
//! CLI  →  Stacker API (enqueue)  →  DB queue  →  Agent polls  →  Agent executes  →  Agent reports
//! ```
//!
//! The CLI never connects to the agent directly. All communication is mediated
//! by the Stacker server.

use crate::cli::config_bundle::build_config_bundle;
use crate::cli::config_parser::StackerConfig;
use crate::cli::error::CliError;
use crate::cli::fmt;
use crate::cli::install_runner::resolve_docker_registry_credentials;
use crate::cli::progress;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{AgentCommandInfo, AgentEnqueueRequest};
use crate::console::commands::CallableTrait;
use std::path::{Path, PathBuf};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Deployment hash resolution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Default poll timeout for agent commands (seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Default poll interval (seconds).
const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;

/// Resolve a deployment hash from explicit flag, active project agent, or deployment lock.
///
/// Resolution order:
/// 1. Explicit `--deployment` flag value
/// 2. `stacker.yml` project name → API project lookup → active agent hash (most reliable)
/// 3. `.stacker/deployment.lock` → `deployment_id` → API lookup for hash (fallback)
fn resolve_deployment_hash(
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    // 1. Explicit flag
    if let Some(hash) = explicit {
        if !hash.is_empty() {
            return Ok(hash.clone());
        }
    }

    let project_dir = std::env::current_dir().map_err(CliError::Io)?;

    // 2. stacker.yml project → active agent (takes priority over lock file)
    // The lock file records the deployment_id at deploy time but the agent may
    // have been redeployed since, leaving the lock pointing at a stale hash.
    let config_path = project_dir.join("stacker.yml");
    if config_path.exists() {
        if let Ok(config) = crate::cli::config_parser::StackerConfig::from_file(&config_path)
            .and_then(|config| config.with_resolved_deploy_target(None))
        {
            if let Some(ref project_name) = config.project.identity {
                if let Ok(Some(proj)) = ctx.block_on(ctx.client.find_project_by_name(project_name))
                {
                    match ctx.block_on(ctx.client.agent_snapshot_by_project(proj.id)) {
                        Ok((_, hash)) => {
                            eprintln!(
                                "\x1b[2mℹ No --deployment specified — using active agent for project '{}': {}\x1b[0m",
                                project_name, hash
                            );
                            return Ok(hash);
                        }
                        Err(_) => {
                            // No active agent for this project; fall through to lock
                        }
                    }
                }
            }
        }
    }

    // 3. Deployment lock (fallback when no stacker.yml or no active project agent)
    if let Some(lock) = crate::cli::deployment_lock::DeploymentLock::load(&project_dir)? {
        if let Some(dep_id) = lock.deployment_id {
            let info = ctx.block_on(ctx.client.get_deployment_status(dep_id as i32))?;
            if let Some(info) = info {
                return Ok(info.deployment_hash);
            }
        }
    }

    Err(CliError::ConfigValidation(
        "Cannot determine deployment hash.\n\
         Use --deployment <HASH>, or run from a directory with a deployment lock or stacker.yml."
            .to_string(),
    ))
}

fn resolve_registry_auth_for_agent_deploy(
    project_dir: &Path,
) -> Option<crate::forms::status_panel::RegistryAuthCommandRequest> {
    let config_path = project_dir.join("stacker.yml");
    let config = crate::cli::config_parser::StackerConfig::from_file(&config_path)
        .and_then(|config| config.with_resolved_deploy_target(None))
        .ok()?;
    let creds = resolve_docker_registry_credentials(&config);
    let username = creds.get("docker_username")?.as_str()?.trim();
    let password = creds.get("docker_password")?.as_str()?.trim();
    if username.is_empty() || password.is_empty() {
        return None;
    }

    let registry = creds
        .get("docker_registry")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("docker.io");

    Some(crate::forms::status_panel::RegistryAuthCommandRequest {
        registry: registry.to_string(),
        username: username.to_string(),
        password: password.to_string(),
    })
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Shared agent command execution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Execute an agent command with spinner and polling.
///
/// 1. Enqueues the command via the Stacker API
/// 2. Shows a spinner while polling for the result
/// 3. Returns the completed `AgentCommandInfo`
fn format_error_message(
    message: &str,
    code: Option<&str>,
    details: Option<&serde_json::Value>,
) -> String {
    let mut formatted = message.to_string();
    if let Some(code) = code.filter(|value| !value.trim().is_empty()) {
        formatted = format!("{} ({})", formatted, code);
    }
    if let Some(details) = details {
        let details = match details {
            serde_json::Value::String(value) => value.clone(),
            other => fmt::pretty_json(other),
        };
        if !details.trim().is_empty() {
            formatted = format!("{}: {}", formatted, details);
        }
    }
    formatted
}

fn json_error_message(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(message) if !message.trim().is_empty() => Some(message.clone()),
        serde_json::Value::Object(map) => {
            if let Some(first) = map
                .get("errors")
                .and_then(|value| value.as_array())
                .and_then(|errors| errors.first())
                .and_then(|value| value.as_object())
            {
                let message = first
                    .get("message")
                    .and_then(|value| value.as_str())
                    .or_else(|| first.get("error").and_then(|value| value.as_str()))
                    .or_else(|| first.get("detail").and_then(|value| value.as_str()))?;
                let code = first.get("code").and_then(|value| value.as_str());
                let details = first.get("details");
                return Some(format_error_message(message, code, details));
            }

            let message = map
                .get("message")
                .and_then(|value| value.as_str())
                .or_else(|| map.get("error").and_then(|value| value.as_str()))
                .or_else(|| map.get("detail").and_then(|value| value.as_str()))?;
            let code = map.get("code").and_then(|value| value.as_str());
            let details = map.get("details");
            Some(format_error_message(message, code, details))
        }
        _ => None,
    }
}

fn agent_command_error_message(info: &AgentCommandInfo) -> Option<String> {
    if let Some(error) = info.error.as_ref() {
        return json_error_message(error).or_else(|| Some(fmt::pretty_json(error)));
    }

    let result = info.result.as_ref()?;
    let reported_status = result.get("status").and_then(|value| value.as_str());
    let result_is_error = matches!(reported_status, Some("error" | "failed"))
        || result.get("success").and_then(|value| value.as_bool()) == Some(false)
        || result.get("ok").and_then(|value| value.as_bool()) == Some(false);

    if !result_is_error {
        return None;
    }

    let mut message = json_error_message(result)
        .unwrap_or_else(|| "Agent command reported an application error".to_string());
    if let Some(code) = result.get("error_code").and_then(|value| value.as_str()) {
        message = format!("{} ({})", message, code);
    }
    Some(message)
}

async fn execute_agent_command(
    ctx: &CliRuntime,
    request: &AgentEnqueueRequest,
    timeout: u64,
) -> Result<AgentCommandInfo, CliError> {
    let info = ctx.client.agent_enqueue(request).await?;
    let command_id = info.command_id.clone();
    let deployment_hash = request.deployment_hash.clone();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    let interval = std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS);
    let mut last_status = "pending".to_string();

    loop {
        tokio::time::sleep(interval).await;

        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::AgentCommandTimeout {
                command_id: command_id.clone(),
                command_type: request.command_type.clone(),
                last_status,
                deployment_hash,
            });
        }

        let status = ctx
            .client
            .agent_command_status(&deployment_hash, &command_id)
            .await?;

        last_status = status.status.clone();

        match status.status.as_str() {
            "completed" => {
                if let Some(error) = agent_command_error_message(&status) {
                    return Err(CliError::AgentCommandFailed {
                        command_id: command_id.clone(),
                        error,
                    });
                }
                return Ok(status);
            }
            "failed" | "cancelled" => {
                let error = agent_command_error_message(&status).unwrap_or_else(|| {
                    format!("Agent command ended with status '{}'", status.status)
                });
                return Err(CliError::AgentCommandFailed {
                    command_id: command_id.clone(),
                    error,
                });
            }
            _ => continue,
        }
    }
}

fn run_agent_command(
    ctx: &CliRuntime,
    request: &AgentEnqueueRequest,
    spinner_msg: &str,
    timeout: u64,
) -> Result<AgentCommandInfo, CliError> {
    let pb = progress::spinner(spinner_msg);

    let result = ctx.block_on(async {
        let info = ctx.client.agent_enqueue(request).await?;
        let command_id = info.command_id.clone();
        let deployment_hash = request.deployment_hash.clone();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
        let interval = std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS);
        let mut last_status = "pending".to_string();

        loop {
            tokio::time::sleep(interval).await;

            if tokio::time::Instant::now() >= deadline {
                return Err(CliError::AgentCommandTimeout {
                    command_id: command_id.clone(),
                    command_type: spinner_msg.to_string(),
                    last_status,
                    deployment_hash,
                });
            }

            let status = ctx
                .client
                .agent_command_status(&deployment_hash, &command_id)
                .await?;

            last_status = status.status.clone();
            progress::update_message(&pb, &format!("{} [{}]", spinner_msg, status.status));

            match status.status.as_str() {
                "completed" => {
                    if let Some(error) = agent_command_error_message(&status) {
                        return Err(CliError::AgentCommandFailed {
                            command_id: command_id.clone(),
                            error,
                        });
                    }
                    return Ok(status);
                }
                "failed" | "cancelled" => {
                    let error = agent_command_error_message(&status).unwrap_or_else(|| {
                        format!("Agent command ended with status '{}'", status.status)
                    });
                    return Err(CliError::AgentCommandFailed {
                        command_id: command_id.clone(),
                        error,
                    });
                }
                _ => continue,
            }
        }
    });

    match &result {
        Ok(_) => progress::finish_success(&pb, spinner_msg),
        Err(e) => {
            let short_msg = match e {
                CliError::AgentCommandTimeout { .. } => {
                    format!("{} — timed out", spinner_msg)
                }
                CliError::AgentCommandFailed { error, .. } => {
                    format!("{} — {}", spinner_msg, error)
                }
                _ => {
                    format!("{} — {}", spinner_msg, e)
                }
            };
            progress::finish_error(&pb, &short_msg);
        }
    }

    result
}

/// Pretty-print an `AgentCommandInfo` result.
fn print_command_result(info: &AgentCommandInfo, json: bool) {
    if json {
        if let Ok(j) = serde_json::to_string_pretty(info) {
            println!("{}", j);
        }
        return;
    }

    println!("Command:  {}", info.command_id);
    println!("Type:     {}", info.command_type);
    println!(
        "Status:   {} {}",
        progress::status_icon(&info.status),
        info.status
    );

    if let Some(ref result) = info.result {
        println!("\n{}", fmt::pretty_json(result));
    }

    if let Some(error) = agent_command_error_message(info) {
        eprintln!("\nError: {}", error);
    }
}

/// Pre-flight connection check for risky agent commands.
///
/// Enqueues a `check_connections` command to the agent and, if active HTTP
/// connections are found, prompts the user interactively. When `force` is
/// `true` the prompt is skipped and execution continues regardless.
///
/// Returns `Ok(())` when it's safe to proceed, or a `CliError` when the user
/// aborts or the prompt cannot be answered.
fn check_active_connections(ctx: &CliRuntime, hash: &str, force: bool) -> Result<(), CliError> {
    let params = crate::forms::status_panel::CheckConnectionsCommandRequest { ports: None };
    let request = AgentEnqueueRequest::new(hash, "check_connections")
        .with_parameters(&params)
        .map_err(|e| CliError::ConfigValidation(format!("check_connections parameters: {}", e)))?;

    let pb = progress::spinner("Checking active connections");
    let info = match ctx.block_on(execute_agent_command(ctx, &request, 15)) {
        Ok(info) => {
            progress::finish_success(&pb, "Checking active connections");
            info
        }
        Err(err) => {
            // Non-fatal: if the check times out or fails we warn but proceed.
            progress::finish_warning(&pb, "Checking active connections — skipped");
            let reason = if matches!(err, CliError::AgentCommandTimeout { .. }) {
                "agent did not respond in time"
            } else {
                "agent could not verify active connections"
            };
            eprintln!("\x1b[33m⚠ Connection check skipped ({})\x1b[0m", reason);
            return Ok(());
        }
    };

    if info.status != "completed" {
        return Ok(());
    }

    let result = match &info.result {
        Some(r) => r.clone(),
        None => return Ok(()),
    };

    let active: u64 = result
        .get("active_connections")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if active == 0 {
        return Ok(());
    }

    // Print a per-port table.
    eprintln!(
        "\n\x1b[33m⚠  {} active HTTP connection(s) detected:\x1b[0m",
        active
    );
    if let Some(ports) = result.get("ports").and_then(|v| v.as_array()) {
        for entry in ports {
            let port = entry.get("port").and_then(|v| v.as_u64()).unwrap_or(0);
            let conns = entry
                .get("connections")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if conns > 0 {
                eprintln!("   port {:5} — {} connection(s)", port, conns);
            }
        }
    }
    eprintln!();

    if force {
        eprintln!("\x1b[2m(--force supplied, proceeding without confirmation)\x1b[0m");
        return Ok(());
    }

    // Interactive prompt.
    eprint!("Proceed anyway? [y/N] ");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(CliError::Io)?;

    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(CliError::ConfigValidation(
            "Aborted: active connections detected. Re-run with --force to skip this check."
                .to_string(),
        )),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Individual agent commands
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// ── Health ───────────────────────────────────────────

/// `stacker agent health [--app <code>] [--json] [--deployment <hash>]`
pub struct AgentHealthCommand {
    pub app_code: Option<String>,
    pub json: bool,
    pub deployment: Option<String>,
    pub include_system: bool,
}

impl AgentHealthCommand {
    pub fn new(
        app_code: Option<String>,
        json: bool,
        deployment: Option<String>,
        include_system: bool,
    ) -> Self {
        Self {
            app_code,
            json,
            deployment,
            include_system,
        }
    }
}

impl CallableTrait for AgentHealthCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent health")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let params = crate::forms::status_panel::HealthCommandRequest {
            app_code: self.app_code.clone().unwrap_or_else(|| "all".to_string()),
            container: None,
            include_metrics: true,
            include_system: self.include_system,
        };

        let request = AgentEnqueueRequest::new(&hash, "health")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(&ctx, &request, "Checking health", DEFAULT_TIMEOUT_SECS)?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Logs ─────────────────────────────────────────────

/// `stacker agent logs [app] [--limit N] [--json] [--deployment <hash>]`
pub struct AgentLogsCommand {
    pub app_code: Option<String>,
    pub limit: Option<i32>,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentLogsCommand {
    pub fn new(
        app_code: Option<String>,
        limit: Option<i32>,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            app_code,
            limit,
            json,
            deployment,
        }
    }
}

impl CallableTrait for AgentLogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent logs")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;
        let limit = self.limit.unwrap_or(400);

        let targets = match &self.app_code {
            Some(app) => vec![app.clone()],
            None => vec!["statuspanel".to_string(), "statuspanel_agent".to_string()],
        };

        let mut results = Vec::new();
        for app_code in targets {
            let info = run_logs_command(&ctx, &hash, &app_code, limit)?;
            if !self.json {
                println!("\n== Logs: {} ==", app_code);
                print_command_result(&info, false);
            }
            results.push(info);
        }

        if self.json {
            let value = serde_json::to_value(&results)
                .map_err(|e| CliError::ConfigValidation(format!("Failed to encode logs: {}", e)))?;
            println!("{}", fmt::pretty_json(&value));
        }
        Ok(())
    }
}

// ── Restart ──────────────────────────────────────────

/// `stacker agent restart <app> [--force] [--json] [--deployment <hash>]`
pub struct AgentRestartCommand {
    pub app_code: String,
    pub force: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentRestartCommand {
    pub fn new(app_code: String, force: bool, json: bool, deployment: Option<String>) -> Self {
        Self {
            app_code,
            force,
            json,
            deployment,
        }
    }
}

impl CallableTrait for AgentRestartCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent restart")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        check_active_connections(&ctx, &hash, self.force)?;

        let params = crate::forms::status_panel::RestartCommandRequest {
            app_code: self.app_code.clone(),
            container: None,
            force: self.force,
        };

        let request = AgentEnqueueRequest::new(&hash, "restart")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Restarting {}", self.app_code),
            DEFAULT_TIMEOUT_SECS,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Deploy App ───────────────────────────────────────

/// `stacker agent deploy-app <app> [--image <img>] [--force] [--runtime <rt>] [--json] [--deployment <hash>]`
pub struct AgentDeployAppCommand {
    pub app_code: String,
    pub image: Option<String>,
    pub force_recreate: bool,
    pub runtime: String,
    pub json: bool,
    pub deployment: Option<String>,
    pub environment: Option<String>,
}

impl AgentDeployAppCommand {
    pub fn new(
        app_code: String,
        image: Option<String>,
        force_recreate: bool,
        runtime: String,
        json: bool,
        deployment: Option<String>,
        environment: Option<String>,
    ) -> Self {
        Self {
            app_code,
            image,
            force_recreate,
            runtime,
            json,
            deployment,
            environment,
        }
    }
}

impl CallableTrait for AgentDeployAppCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent deploy-app")?;
        let project_dir = std::env::current_dir().map_err(CliError::Io)?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        check_active_connections(&ctx, &hash, self.force_recreate)?;
        let local_config = local_config_files_for_agent_deploy(
            &project_dir,
            &self.app_code,
            self.environment.as_deref(),
        )?;
        for notice in &local_config.notices {
            eprintln!("  ⚠ {notice}");
        }

        let params = crate::forms::status_panel::DeployAppCommandRequest {
            app_code: self.app_code.clone(),
            compose_content: local_config.compose_content,
            image: self.image.clone(),
            env_vars: None,
            pull: true,
            force_recreate: self.force_recreate,
            force_config_overwrite: self.force_recreate,
            runtime: self.runtime.clone(),
            registry_auth: resolve_registry_auth_for_agent_deploy(&project_dir),
            config_files: local_config.config_files,
        };

        let request = AgentEnqueueRequest::new(&hash, "deploy_app")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?
            .with_timeout(300);

        let info = run_agent_command(&ctx, &request, &format!("Deploying {}", self.app_code), 300)?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

#[derive(Debug, Default)]
struct LocalDeployAppConfig {
    compose_content: Option<String>,
    config_files: Option<Vec<serde_json::Value>>,
    notices: Vec<String>,
}

fn local_config_files_for_agent_deploy(
    project_dir: &Path,
    app_code: &str,
    environment_override: Option<&str>,
) -> Result<LocalDeployAppConfig, CliError> {
    let mut result = LocalDeployAppConfig::default();
    let config_path = project_dir.join("stacker.yml");
    if !config_path.exists() {
        return Ok(result);
    }

    let active_target =
        crate::cli::deployment_lock::DeploymentLock::read_active_target(project_dir)?;
    let mut config = StackerConfig::from_file(&config_path)?
        .with_resolved_deploy_target(active_target.as_deref())?;
    let active_environment = read_active_environment(project_dir)?;
    let requested_environment = environment_override.or(active_environment.as_deref());
    let Some((environment, environment_config)) =
        config.resolve_environment_config(requested_environment)?
    else {
        return Ok(result);
    };

    if environment_override.is_none() && active_environment.is_none() {
        if let Some(active_target) = active_target.as_deref() {
            if active_target != "local" && environment == "local" {
                result.notices.push(format!(
                    "Active target is '{}', but resolved environment is 'local'; use `stacker agent deploy-app {} --env prod` or `stacker env prod` if this should use production config.",
                    active_target, app_code
                ));
            }
        }
    }

    if let Some(compose_file) = environment_config.compose_file {
        config.deploy.compose_file = Some(compose_file);
    }
    if let Some(env_file) = environment_config.env_file {
        config.env_file = Some(env_file);
    }

    let Some(configured_compose_file) = config.deploy.compose_file.as_ref() else {
        return Ok(result);
    };
    let compose_path = resolve_deploy_app_compose_path(
        project_dir,
        app_code,
        &environment,
        configured_compose_file,
    );
    if !compose_path.exists() {
        return Ok(result);
    }

    if !compose_service_has_env_file(&compose_path, app_code)? {
        let conventional_env = project_dir
            .join(app_code)
            .join("docker")
            .join(&environment)
            .join(".env");
        if conventional_env.exists() {
            result.notices.push(format!(
                "{} exists, but service '{}' in {} has no env_file entry; Docker Compose will not inject local or remote-rendered env values into that container.",
                conventional_env.display(),
                app_code,
                compose_path.display()
            ));
        }
    }

    let bundle = build_config_bundle(
        project_dir,
        &environment,
        &compose_path,
        config.env_file.as_deref(),
    )?;

    result.compose_content = bundle
        .config_files
        .iter()
        .find(|file| file.get("name").and_then(|name| name.as_str()) == Some("docker-compose.yml"))
        .and_then(|file| file.get("content").and_then(|content| content.as_str()))
        .map(ToOwned::to_owned);
    let absolute_config_files: Vec<_> = bundle
        .config_files
        .into_iter()
        .filter(|file| {
            file.get("destination_path")
                .and_then(|path| path.as_str())
                .map(|path| path.starts_with("/opt/stacker/deployments/"))
                .unwrap_or(false)
        })
        .collect();
    if !absolute_config_files.is_empty() {
        result.config_files = Some(absolute_config_files);
    }
    Ok(result)
}

fn resolve_deploy_app_compose_path(
    project_dir: &Path,
    app_code: &str,
    environment: &str,
    configured_compose_file: &Path,
) -> PathBuf {
    let app_local_compose = project_dir
        .join(app_code)
        .join("docker")
        .join(environment)
        .join("compose.yml");
    if app_local_compose.exists() {
        return app_local_compose;
    }

    if configured_compose_file.is_absolute() {
        configured_compose_file.to_path_buf()
    } else {
        project_dir.join(configured_compose_file)
    }
}

fn active_environment_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".stacker").join("active-env")
}

fn read_active_environment(project_dir: &Path) -> Result<Option<String>, CliError> {
    let path = active_environment_path(project_dir);
    if !path.exists() {
        return Ok(None);
    }

    let value = std::fs::read_to_string(path).map_err(CliError::Io)?;
    let value = value.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn compose_service_has_env_file(compose_path: &Path, app_code: &str) -> Result<bool, CliError> {
    let raw = std::fs::read_to_string(compose_path).map_err(CliError::Io)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw)?;
    let Some(service) = doc
        .as_mapping()
        .and_then(|root| root.get(serde_yaml::Value::String("services".to_string())))
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|services| services.get(serde_yaml::Value::String(app_code.to_string())))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return Ok(false);
    };

    Ok(service
        .get(serde_yaml::Value::String("env_file".to_string()))
        .is_some())
}

// ── Remove App ───────────────────────────────────────

/// `stacker agent remove-app <app> [--volumes] [--force] [--json] [--deployment <hash>]`
pub struct AgentRemoveAppCommand {
    pub app_code: String,
    pub remove_volumes: bool,
    pub remove_image: bool,
    pub force: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentRemoveAppCommand {
    pub fn new(
        app_code: String,
        remove_volumes: bool,
        remove_image: bool,
        force: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            app_code,
            remove_volumes,
            remove_image,
            force,
            json,
            deployment,
        }
    }
}

impl CallableTrait for AgentRemoveAppCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent remove-app")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        check_active_connections(&ctx, &hash, self.force)?;

        let params = crate::forms::status_panel::RemoveAppCommandRequest {
            app_code: self.app_code.clone(),
            delete_config: true,
            remove_volumes: self.remove_volumes,
            remove_image: self.remove_image,
        };

        let request = AgentEnqueueRequest::new(&hash, "remove_app")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Removing {}", self.app_code),
            DEFAULT_TIMEOUT_SECS,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Configure Firewall ───────────────────────────────

/// `stacker agent configure-firewall [--action add] [--public-ports 80/tcp,443/tcp] [--private-ports 5432/tcp:10.0.0.0/8] [--force] [--json] [--deployment <hash>]`
pub struct AgentConfigureFirewallCommand {
    pub action: String,
    pub app_code: Option<String>,
    pub public_ports: Vec<String>,
    pub private_ports: Vec<String>,
    pub persist: bool,
    pub force: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentConfigureFirewallCommand {
    pub fn new(
        action: String,
        app_code: Option<String>,
        public_ports: Vec<String>,
        private_ports: Vec<String>,
        persist: bool,
        force: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            action,
            app_code,
            public_ports,
            private_ports,
            persist,
            force,
            json,
            deployment,
        }
    }

    fn parse_public_port(s: &str) -> Result<crate::forms::status_panel::FirewallPortRule, String> {
        crate::forms::firewall::parse_public_port(s)
    }

    fn parse_private_port(s: &str) -> Result<crate::forms::status_panel::FirewallPortRule, String> {
        crate::forms::firewall::parse_private_port(s)
    }
}

impl CallableTrait for AgentConfigureFirewallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent configure-firewall")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        check_active_connections(&ctx, &hash, self.force)?;

        let public: Vec<crate::forms::status_panel::FirewallPortRule> = self
            .public_ports
            .iter()
            .map(|s| Self::parse_public_port(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CliError::ConfigValidation(e))?;

        let private: Vec<crate::forms::status_panel::FirewallPortRule> = self
            .private_ports
            .iter()
            .map(|s| Self::parse_private_port(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CliError::ConfigValidation(e))?;

        let params = crate::forms::status_panel::ConfigureFirewallCommandRequest {
            app_code: self.app_code.clone(),
            public_ports: public,
            private_ports: private,
            action: self.action.clone(),
            persist: self.persist,
        };

        let request = AgentEnqueueRequest::new(&hash, "configure_firewall")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Configuring firewall ({})", self.action),
            DEFAULT_TIMEOUT_SECS,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Configure Proxy ──────────────────────────────────

/// `stacker agent configure-proxy <app> --domain <d> --port <p> [--no-ssl] [--force] [--json] [--deployment <hash>]`
pub struct AgentConfigureProxyCommand {
    pub app_code: String,
    pub domain: String,
    pub port: u16,
    pub ssl: bool,
    pub action: String,
    pub force: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentConfigureProxyCommand {
    pub fn new(
        app_code: String,
        domain: String,
        port: u16,
        ssl: bool,
        no_ssl: bool,
        action: String,
        force: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        let ssl = ssl && !no_ssl;
        Self {
            app_code,
            domain,
            port,
            ssl,
            action,
            force,
            json,
            deployment,
        }
    }
}

impl CallableTrait for AgentConfigureProxyCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent configure-proxy")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        check_active_connections(&ctx, &hash, self.force)?;

        let params = crate::forms::status_panel::ConfigureProxyCommandRequest {
            app_code: self.app_code.clone(),
            domain_names: vec![self.domain.clone()],
            forward_host: None,
            forward_port: self.port,
            ssl_enabled: self.ssl,
            ssl_forced: self.ssl,
            http2_support: self.ssl,
            action: self.action.clone(),
        };

        let request = AgentEnqueueRequest::new(&hash, "configure_proxy")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Configuring proxy for {}", self.app_code),
            DEFAULT_TIMEOUT_SECS,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Status / Snapshot ────────────────────────────────

/// `stacker agent status [--json] [--deployment <hash>]`
///
/// Fetches the full deployment snapshot: agent info, recent commands,
/// container states.
pub struct AgentStatusCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentStatusCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

impl CallableTrait for AgentStatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent status")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let pb = progress::spinner("Fetching agent status");

        let snapshot = ctx.block_on(ctx.client.agent_snapshot(&hash));

        match snapshot {
            Ok(snap) => {
                let item = snapshot_item(&snap);

                let agent_status = item
                    .get("agent")
                    .and_then(|a| a.get("status"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let version = item
                    .get("agent")
                    .and_then(|a| a.get("version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let n_apps = item
                    .get("apps")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);

                progress::finish_success(
                    &pb,
                    &format!(
                        "Agent status fetched — {} {} · v{} · {} app(s)",
                        progress::status_icon(agent_status),
                        agent_status,
                        version,
                        n_apps,
                    ),
                );
                let live_containers = match fetch_live_containers(&ctx, &hash) {
                    Ok(list) => list,
                    Err(err) => {
                        eprintln!("Warning: failed to fetch live containers: {}", err);
                        None
                    }
                };

                if self.json {
                    let mut output = item.clone();
                    if let Some(list) = &live_containers {
                        if let Some(obj) = output.as_object_mut() {
                            obj.insert(
                                "containers_live".to_string(),
                                serde_json::Value::Array(list.clone()),
                            );
                        } else {
                            output = serde_json::json!({
                                "snapshot": output,
                                "containers_live": list,
                            });
                        }
                    }
                    println!("{}", fmt::pretty_json(&output));
                } else {
                    print_snapshot_summary(item, live_containers.as_ref());
                }
            }
            Err(e) => {
                progress::finish_error(&pb, &format!("Failed: {}", e));
                return Err(Box::new(e));
            }
        }

        Ok(())
    }
}

fn snapshot_item<'a>(snap: &'a serde_json::Value) -> &'a serde_json::Value {
    snap.get("item").unwrap_or(snap)
}

fn print_apps_summary(apps: &[serde_json::Value]) {
    if apps.is_empty() {
        println!("Apps:       none");
        return;
    }

    println!("{:<18} {:<22} {:<30} {}", "APP", "NAME", "IMAGE", "ENABLED");
    for app in apps {
        let code = app.get("code").and_then(|v| v.as_str()).unwrap_or("-");
        let name = app.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let image = app.get("image").and_then(|v| v.as_str()).unwrap_or("-");
        let enabled = app.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        println!(
            "{:<18} {:<22} {:<30} {}",
            fmt::truncate(code, 16),
            fmt::truncate(name, 20),
            fmt::truncate(image, 28),
            if enabled { "yes" } else { "no" }
        );
    }
}

fn print_containers_summary(containers: &[serde_json::Value]) {
    let containers = visible_containers(containers);

    if containers.is_empty() {
        println!("Containers: none");
        return;
    }

    println!("{:<24} {:<12} {:<30}", "CONTAINER", "STATE", "IMAGE");
    for c in containers {
        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let state = c
            .get("state")
            .or_else(|| c.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let image = c.get("image").and_then(|v| v.as_str()).unwrap_or("-");
        println!(
            "{:<24} {} {:<10} {:<30}",
            fmt::truncate(name, 22),
            progress::status_icon(state),
            state,
            fmt::truncate(image, 28),
        );
    }
}

fn visible_containers(containers: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    containers
        .iter()
        .filter(|container| !is_stale_platform_project_container(container))
        .collect()
}

fn is_stale_platform_project_container(container: &serde_json::Value) -> bool {
    let Some(name) = container.get("name").and_then(|value| value.as_str()) else {
        return false;
    };

    let normalized_name = crate::project_app::normalize_app_code(name);
    normalized_name.starts_with("project_")
        && ["nginx_proxy_manager", "statuspanel"]
            .iter()
            .any(|code| normalized_name.contains(code))
}

/// Pretty-print a snapshot summary for human consumption.
fn print_snapshot_summary(
    snap: &serde_json::Value,
    live_containers: Option<&Vec<serde_json::Value>>,
) {
    println!("{}", fmt::separator(60));

    // Agent info
    if let Some(agent) = snap.get("agent") {
        let status = agent
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let version = agent.get("version").and_then(|v| v.as_str()).unwrap_or("-");
        let heartbeat = agent
            .get("last_heartbeat")
            .and_then(|v| v.as_str())
            .unwrap_or("-");

        println!(
            "Agent:     {} {}  (v{})",
            progress::status_icon(status),
            status,
            version
        );
        println!("Heartbeat: {}", heartbeat);
    } else {
        println!("Agent:     not registered");
    }

    println!("{}", fmt::separator(60));

    if let Some(apps) = snap.get("apps").and_then(|v| v.as_array()) {
        print_apps_summary(apps);
    } else {
        println!("Apps:       none");
    }

    println!("{}", fmt::separator(60));

    // Containers
    if let Some(containers) = live_containers {
        print_containers_summary(containers);
    } else if let Some(containers) = snap.get("containers").and_then(|v| v.as_array()) {
        print_containers_summary(containers);
    }

    println!("{}", fmt::separator(60));

    // Recent commands
    if let Some(commands) = snap.get("commands").and_then(|v| v.as_array()) {
        let recent: Vec<_> = commands.iter().take(5).collect();
        if recent.is_empty() {
            println!("Recent commands: none");
        } else {
            println!(
                "{:<24} {:<14} {:<10} {}",
                "COMMAND", "TYPE", "STATUS", "CREATED"
            );
            for c in &recent {
                let id = c.get("command_id").and_then(|v| v.as_str()).unwrap_or("-");
                let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("-");
                let status = c.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                let created = c.get("created_at").and_then(|v| v.as_str()).unwrap_or("-");
                println!(
                    "{:<24} {:<14} {} {:<8} {}",
                    fmt::truncate(id, 22),
                    ctype,
                    progress::status_icon(status),
                    status,
                    fmt::truncate(created, 19),
                );
            }
        }
    }
}

pub struct AgentListAppsCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentListAppsCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

impl CallableTrait for AgentListAppsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent list apps")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let snapshot = ctx.block_on(ctx.client.agent_snapshot(&hash))?;
        let item = snapshot_item(&snapshot);
        let apps = item
            .get("apps")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if self.json {
            let value = serde_json::Value::Array(apps);
            println!("{}", fmt::pretty_json(&value));
            return Ok(());
        }

        print_apps_summary(&apps);
        Ok(())
    }
}

pub struct AgentListContainersCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentListContainersCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

impl CallableTrait for AgentListContainersCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent list containers")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let containers = fetch_live_containers(&ctx, &hash)?.unwrap_or_default();

        if self.json {
            let value = serde_json::Value::Array(containers);
            println!("{}", fmt::pretty_json(&value));
            return Ok(());
        }

        print_containers_summary(&containers);
        Ok(())
    }
}

fn run_logs_command(
    ctx: &CliRuntime,
    deployment_hash: &str,
    app_code: &str,
    limit: i32,
) -> Result<AgentCommandInfo, CliError> {
    let params = crate::forms::status_panel::LogsCommandRequest {
        app_code: app_code.to_string(),
        container: None,
        cursor: None,
        limit,
        streams: vec!["stdout".to_string(), "stderr".to_string()],
        redact: true,
    };

    let request = AgentEnqueueRequest::new(deployment_hash, "logs")
        .with_parameters(&params)
        .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

    run_agent_command(
        ctx,
        &request,
        &format!("Fetching logs ({})", app_code),
        DEFAULT_TIMEOUT_SECS,
    )
}

fn fetch_live_containers(
    ctx: &CliRuntime,
    deployment_hash: &str,
) -> Result<Option<Vec<serde_json::Value>>, CliError> {
    let params = crate::forms::status_panel::ListContainersCommandRequest {
        include_health: true,
        include_logs: false,
        log_lines: 10,
    };

    let request = AgentEnqueueRequest::new(deployment_hash, "list_containers")
        .with_parameters(&params)
        .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

    let info = run_agent_command(ctx, &request, "Fetching containers", DEFAULT_TIMEOUT_SECS)?;
    if info.status != "completed" {
        return Ok(None);
    }

    let containers = info
        .result
        .and_then(|result| result.get("containers").and_then(|v| v.as_array()).cloned());
    Ok(containers)
}

// ── Exec (raw command) ───────────────────────────────

/// `stacker agent exec <command_type> [--params <json>] [--json] [--deployment <hash>]`
///
/// Low-level command for sending arbitrary command types to the agent.
pub struct AgentExecCommand {
    pub command_type: String,
    pub params: Option<String>,
    pub timeout: Option<u64>,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentExecCommand {
    pub fn new(
        command_type: String,
        params: Option<String>,
        timeout: Option<u64>,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            command_type,
            params,
            timeout,
            json,
            deployment,
        }
    }
}

impl CallableTrait for AgentExecCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent exec")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let mut request = AgentEnqueueRequest::new(&hash, &self.command_type);

        if let Some(ref params_str) = self.params {
            let value: serde_json::Value = serde_json::from_str(params_str).map_err(|e| {
                CliError::ConfigValidation(format!("Invalid JSON parameters: {}", e))
            })?;
            request = request.with_raw_parameters(value);
        }

        let timeout = self.timeout.unwrap_or(DEFAULT_TIMEOUT_SECS);
        if let Some(t) = self.timeout {
            request = request.with_timeout(t as i32);
        }

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Executing {}", self.command_type),
            timeout,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Command History ──────────────────────────────────

/// `stacker agent history [--json] [--deployment <hash>]`
///
/// Shows recent commands sent to the agent via the snapshot endpoint.
pub struct AgentHistoryCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentHistoryCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

impl CallableTrait for AgentHistoryCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent history")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let snap = ctx.block_on(ctx.client.agent_snapshot(&hash))?;

        if self.json {
            if let Some(commands) = snap.get("commands") {
                println!("{}", fmt::pretty_json(commands));
            } else {
                println!("[]");
            }
            return Ok(());
        }

        if let Some(commands) = snap.get("commands").and_then(|v| v.as_array()) {
            if commands.is_empty() {
                println!("No commands found.");
                return Ok(());
            }

            println!(
                "{:<24} {:<14} {:<10} {:<10} {}",
                "COMMAND", "TYPE", "STATUS", "PRIORITY", "CREATED"
            );
            println!("{}", fmt::separator(80));

            for c in commands {
                let id = c.get("command_id").and_then(|v| v.as_str()).unwrap_or("-");
                let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("-");
                let status = c.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                let priority = c.get("priority").and_then(|v| v.as_str()).unwrap_or("-");
                let created = c.get("created_at").and_then(|v| v.as_str()).unwrap_or("-");
                println!(
                    "{:<24} {:<14} {} {:<8} {:<10} {}",
                    fmt::truncate(id, 22),
                    ctype,
                    progress::status_icon(status),
                    status,
                    priority,
                    fmt::truncate(created, 19),
                );
            }
        } else {
            println!("No commands found.");
        }

        Ok(())
    }
}

// ── Install (deploy Status Panel to existing server) ─

/// `stacker agent install [--file <path>] [--json]`
///
/// Deploys the Status Panel agent to an existing server that was previously
/// deployed without it. Reads the project identity from stacker.yml, finds
/// the corresponding project and server on the Stacker API, and triggers
/// a deploy with only the statuspanel feature enabled.
pub struct AgentInstallCommand {
    pub file: Option<String>,
    pub json: bool,
}

impl AgentInstallCommand {
    pub fn new(file: Option<String>, json: bool) -> Self {
        Self { file, json }
    }
}

fn fallback_server_config_for_agent_install(
    server: &crate::cli::stacker_client::ServerInfo,
) -> Result<crate::cli::config_parser::ServerConfig, CliError> {
    let host = server.srv_ip.clone().ok_or_else(|| {
        CliError::ConfigValidation(
            "Server record has no reachable IP address.\n\
             Cannot install Status Panel without a server host."
                .to_string(),
        )
    })?;

    let port = server
        .ssh_port
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(22);

    Ok(crate::cli::config_parser::ServerConfig {
        host,
        user: server
            .ssh_user
            .clone()
            .unwrap_or_else(|| "root".to_string()),
        ssh_key: None,
        port,
    })
}

fn build_agent_install_deploy_request(
    config: &crate::cli::config_parser::StackerConfig,
    server: &crate::cli::stacker_client::ServerInfo,
    project_name: &str,
    vault_url: &str,
) -> Result<(Option<i32>, serde_json::Value), CliError> {
    let server_target = config.deploy.target == crate::cli::config_parser::DeployTarget::Server
        || server.cloud_id.is_none();

    if server_target {
        let server_cfg = match config.deploy.server.as_ref() {
            Some(server_cfg) => server_cfg.clone(),
            None => fallback_server_config_for_agent_install(server)?,
        };
        let effective_server_name = server
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-server", project_name));
        let mut deploy_form = crate::cli::stacker_client::build_server_deploy_form(
            config,
            &server_cfg,
            &effective_server_name,
            true,
        );

        if let Some(server_obj) = deploy_form
            .get_mut("server")
            .and_then(|value| value.as_object_mut())
        {
            if let Some((private_key, public_key)) =
                crate::cli::install_runner::load_existing_server_ssh_key(&server_cfg)?
            {
                server_obj.insert(
                    "ssh_private_key".to_string(),
                    serde_json::Value::String(private_key),
                );
                if let Some(public_key) = public_key {
                    server_obj.insert(
                        "public_key".to_string(),
                        serde_json::Value::String(public_key),
                    );
                }
            }

            server_obj.insert("server_id".to_string(), serde_json::json!(server.id));

            if let Some(vault_key_path) = &server.vault_key_path {
                server_obj.insert(
                    "vault_key_path".to_string(),
                    serde_json::Value::String(vault_key_path.clone()),
                );
            }

            if let Some(region) = &server.region {
                server_obj.insert(
                    "region".to_string(),
                    serde_json::Value::String(region.clone()),
                );
            }

            if let Some(os) = &server.os {
                server_obj.insert("os".to_string(), serde_json::Value::String(os.clone()));
            }

            if let Some(server_kind) = &server.server {
                server_obj.insert(
                    "server".to_string(),
                    serde_json::Value::String(server_kind.clone()),
                );
            }
        }

        return Ok((None, deploy_form));
    }

    let cloud_id = server.cloud_id.ok_or_else(|| {
        CliError::ConfigValidation(
            "Server has no associated cloud credentials.\n\
             Cannot install Status Panel without cloud credentials."
                .to_string(),
        )
    })?;

    let deploy_form = serde_json::json!({
        "cloud": {
            "provider": server.cloud.clone().unwrap_or_else(|| "htz".to_string()),
            "save_token": true,
        },
        "server": {
            "server_id": server.id,
            "region": server.region,
            "server": server.server,
            "os": server.os,
            "name": server.name,
            "srv_ip": server.srv_ip,
            "ssh_user": server.ssh_user,
            "ssh_port": server.ssh_port,
            "vault_key_path": server.vault_key_path,
            "connection_mode": "status_panel",
        },
        "stack": {
            "stack_code": project_name,
            "vars": [
                { "key": "vault_url", "value": vault_url },
                { "key": "status_panel_port", "value": "5000" },
            ],
            "integrated_features": ["statuspanel"],
            "extended_features": [],
            "subscriptions": [],
        },
    });

    Ok((Some(cloud_id), deploy_form))
}

impl CallableTrait for AgentInstallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::cli::stacker_client::{self, DEFAULT_VAULT_URL};

        let project_dir = std::env::current_dir().map_err(CliError::Io)?;
        let config_path = match &self.file {
            Some(f) => project_dir.join(f),
            None => project_dir.join("stacker.yml"),
        };

        let config = crate::cli::config_parser::StackerConfig::from_file(&config_path)?
            .with_resolved_deploy_target(None)?;

        let project_name = config
            .project
            .identity
            .clone()
            .unwrap_or_else(|| config.name.clone());

        let ctx = CliRuntime::new("agent install")?;
        let pb = progress::spinner("Installing Status Panel agent");

        let result: Result<stacker_client::DeployResponse, CliError> = ctx.block_on(async {
            let target_label = config.deploy.target.to_string();
            // 1. Find the project
            progress::update_message(&pb, "Finding project...");
            let project = ctx
                .client
                .find_project_by_name(&project_name)
                .await?
                .ok_or_else(|| {
                    CliError::ConfigValidation(format!(
                        "Project '{}' not found on the Stacker server.\n\
                     Deploy the project first with: stacker deploy --target {}",
                        project_name, target_label
                    ))
                })?;

            // 2. Find the server for this project
            progress::update_message(&pb, "Finding server...");
            let servers = ctx.client.list_servers().await?;
            let server = servers
                .into_iter()
                .find(|s| s.project_id == project.id)
                .ok_or_else(|| {
                    CliError::ConfigValidation(format!(
                        "No server found for project '{}' (id={}).\n\
                     Deploy the project first with: stacker deploy --target {}",
                        project_name, project.id, target_label
                    ))
                })?;

            // 3. Build a minimal deploy form with only the statuspanel feature
            progress::update_message(&pb, "Preparing deploy payload...");
            let vault_url = std::env::var("STACKER_VAULT_URL")
                .unwrap_or_else(|_| DEFAULT_VAULT_URL.to_string());
            let (cloud_id, deploy_form) =
                build_agent_install_deploy_request(&config, &server, &project_name, &vault_url)?;

            // 4. Trigger the deploy
            progress::update_message(&pb, "Deploying Status Panel...");
            let resp = ctx.client.deploy(project.id, cloud_id, deploy_form).await?;
            Ok(resp)
        });

        match result {
            Ok(resp) => {
                progress::finish_success(&pb, "Status Panel agent installation triggered");

                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&resp).unwrap_or_default()
                    );
                } else {
                    println!("Status Panel deploy queued for project '{}'", project_name);
                    if let Some(id) = resp.id {
                        println!("Project ID: {}", id);
                    }
                    if let Some(meta) = &resp.meta {
                        if let Some(dep_id) = meta.get("deployment_id") {
                            println!("Deployment ID: {}", dep_id);
                        }
                    }
                    println!();
                    println!("The Status Panel agent will be installed on the server.");
                    println!("Once ready, use `stacker agent status` to verify connectivity.");
                }
            }
            Err(e) => {
                progress::finish_error(&pb, &format!("Install failed: {}", e));
                return Err(Box::new(e));
            }
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
    use tempfile::TempDir;

    fn sample_server_info() -> crate::cli::stacker_client::ServerInfo {
        crate::cli::stacker_client::ServerInfo {
            id: 7,
            user_id: "user_1".to_string(),
            project_id: 42,
            cloud_id: None,
            cloud: None,
            region: Some("nbg1".to_string()),
            zone: None,
            server: Some("cpx11".to_string()),
            os: Some("ubuntu-24.04".to_string()),
            disk_type: None,
            srv_ip: Some("203.0.113.10".to_string()),
            ssh_port: Some(2222),
            ssh_user: Some("deployer".to_string()),
            name: Some("syncopia-prod".to_string()),
            vault_key_path: Some("secret/users/user_1/servers/7/ssh".to_string()),
            connection_mode: "ssh".to_string(),
            key_status: "uploaded".to_string(),
        }
    }

    #[test]
    fn compose_service_has_env_file_detects_service_topology() {
        let dir = TempDir::new().expect("temp dir");
        let compose_path = dir.path().join("compose.yml");
        std::fs::write(
            &compose_path,
            r#"
services:
  device-api:
    image: syncopia/device-api:latest
    env_file:
      - ../../device-api/docker/prod/.env
  upload:
    image: syncopia/upload:latest
"#,
        )
        .expect("compose");

        assert!(compose_service_has_env_file(&compose_path, "device-api").unwrap());
        assert!(!compose_service_has_env_file(&compose_path, "upload").unwrap());
    }

    #[test]
    fn local_config_files_warns_when_conventional_env_is_not_in_compose_topology() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("docker/prod")).expect("docker prod");
        std::fs::create_dir_all(root.join("device-api/docker/prod")).expect("service env dir");
        std::fs::write(
            root.join("docker/prod/.env"),
            "DEVICE_API_IMAGE=syncopia/device-api\n",
        )
        .expect("project env");
        std::fs::write(root.join("device-api/docker/prod/.env"), "RUST_LOG=debug\n")
            .expect("service env");
        std::fs::write(
            root.join("docker/prod/compose.yml"),
            r#"
services:
  device-api:
    image: ${DEVICE_API_IMAGE}
"#,
        )
        .expect("compose");
        std::fs::write(
            root.join("stacker.yml"),
            r#"
name: syncopia
project:
  identity: syncopia
app:
  image: syncopia/device-api:latest
deploy:
  target: server
  environment: prod
  server:
    host: 203.0.113.10
environments:
  prod:
    compose_file: docker/prod/compose.yml
    env_file: docker/prod/.env
"#,
        )
        .expect("stacker config");

        let config = local_config_files_for_agent_deploy(root, "device-api", None).unwrap();

        assert!(config.config_files.is_some());
        assert!(config.compose_content.is_some());
        assert_eq!(config.notices.len(), 1);
        assert!(config.notices[0].contains("has no env_file entry"));
    }

    #[test]
    fn local_config_files_uses_environment_override() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("docker/local")).expect("docker local");
        std::fs::create_dir_all(root.join("docker/prod")).expect("docker prod");
        std::fs::write(
            root.join("docker/local/compose.yml"),
            "services:\n  device-api:\n    image: syncopia/device-api:local\n",
        )
        .expect("local compose");
        std::fs::write(
            root.join("docker/prod/compose.yml"),
            "services:\n  device-api:\n    image: syncopia/device-api:prod\n",
        )
        .expect("prod compose");
        std::fs::write(
            root.join("stacker.yml"),
            r#"
name: syncopia
project:
  identity: syncopia
app:
  image: syncopia/device-api:latest
deploy:
  target: local
  environment: local
environments:
  local:
    compose_file: docker/local/compose.yml
  prod:
    compose_file: docker/prod/compose.yml
"#,
        )
        .expect("stacker config");

        let config = local_config_files_for_agent_deploy(root, "device-api", Some("prod")).unwrap();

        let compose = config.compose_content.expect("compose content");
        assert!(compose.contains("syncopia/device-api:prod"));
        assert!(!compose.contains("syncopia/device-api:local"));
    }

    #[test]
    fn local_config_files_prefers_app_local_compose_for_deploy_app() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("docker/prod")).expect("project compose dir");
        std::fs::create_dir_all(root.join("device-api/docker/prod")).expect("app compose dir");
        std::fs::write(
            root.join("docker/prod/compose.yml"),
            "services:\n  database:\n    image: postgres:17-alpine\n",
        )
        .expect("project compose");
        std::fs::write(root.join("device-api/docker/prod/.env"), "RUST_LOG=debug\n")
            .expect("app env");
        std::fs::write(
            root.join("device-api/docker/prod/compose.yml"),
            "services:\n  device-api:\n    image: syncopia/device-api:prod\n    env_file: .env\n",
        )
        .expect("app compose");
        std::fs::write(
            root.join("stacker.yml"),
            r#"
name: syncopia
project:
  identity: syncopia
app:
  image: syncopia/device-api:latest
deploy:
  target: server
  environment: prod
environments:
  prod:
    compose_file: docker/prod/compose.yml
"#,
        )
        .expect("stacker config");

        let config = local_config_files_for_agent_deploy(root, "device-api", None).unwrap();

        let compose = config.compose_content.expect("compose content");
        assert!(compose.contains("syncopia/device-api:prod"));
        assert!(!compose.contains("postgres:17-alpine"));
        assert!(config.notices.is_empty());
        let config_files = config.config_files.expect("config files");
        assert!(config_files.iter().any(|file| {
            file.get("destination_path")
                .and_then(|path| path.as_str())
                .map(|path| path.ends_with("/device-api/docker/prod/.env"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn enqueue_request_builder() {
        let req = AgentEnqueueRequest::new("abc123", "health")
            .with_priority("high")
            .with_timeout(120);

        assert_eq!(req.deployment_hash, "abc123");
        assert_eq!(req.command_type, "health");
        assert_eq!(req.priority, Some("high".to_string()));
        assert_eq!(req.timeout_seconds, Some(120));
    }

    #[test]
    fn enqueue_request_with_typed_params() {
        let params = crate::forms::status_panel::HealthCommandRequest {
            app_code: "myapp".to_string(),
            container: None,
            include_metrics: true,
            include_system: false,
        };

        let req = AgentEnqueueRequest::new("hash", "health")
            .with_parameters(&params)
            .expect("serialization should succeed");

        assert!(req.parameters.is_some());
        let p = req.parameters.unwrap();
        assert_eq!(p["app_code"], "myapp");
    }

    #[test]
    fn print_snapshot_summary_handles_empty() {
        let snap = serde_json::json!({});
        // Should not panic
        print_snapshot_summary(&snap, None);
    }

    #[test]
    fn visible_containers_hides_stale_platform_project_container() {
        let containers = vec![
            serde_json::json!({
                "name": "nginx-proxy-manager",
                "state": "running",
                "image": "jc21/nginx-proxy-manager:latest"
            }),
            serde_json::json!({
                "name": "project-nginx_proxy_manager-1",
                "state": "exited",
                "image": "jc21/nginx-proxy-manager:latest"
            }),
            serde_json::json!({
                "name": "project-coolify-1",
                "state": "running",
                "image": "coollabsio/coolify:latest"
            }),
        ];

        let visible = visible_containers(&containers);
        let names = visible
            .iter()
            .map(|container| container["name"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["nginx-proxy-manager", "project-coolify-1"]);
    }

    #[test]
    fn agent_install_request_uses_server_deploy_path_without_cloud_credentials() {
        let config = crate::cli::config_parser::ConfigBuilder::new()
            .name("syncopia")
            .deploy_target(crate::cli::config_parser::DeployTarget::Server)
            .build()
            .expect("config");
        let server = sample_server_info();

        let (cloud_id, deploy_form) = build_agent_install_deploy_request(
            &config,
            &server,
            "syncopia",
            "https://vault.try.direct",
        )
        .expect("server install request");

        assert_eq!(cloud_id, None);
        assert_eq!(deploy_form["cloud"]["provider"], "own");
        assert_eq!(deploy_form["server"]["server_id"], 7);
        assert_eq!(deploy_form["server"]["srv_ip"], "203.0.113.10");
        assert_eq!(deploy_form["server"]["ssh_user"], "deployer");
        assert_eq!(deploy_form["server"]["ssh_port"], 2222);
        assert_eq!(deploy_form["server"]["connection_mode"], "status_panel");
        assert_eq!(
            deploy_form["server"]["vault_key_path"],
            "secret/users/user_1/servers/7/ssh"
        );
        assert!(deploy_form["stack"]["integrated_features"]
            .as_array()
            .expect("integrated_features array")
            .contains(&serde_json::Value::String("statuspanel".to_string())));
    }

    #[test]
    fn agent_install_request_keeps_cloud_deploy_path_when_cloud_server_is_linked() {
        let config = crate::cli::config_parser::ConfigBuilder::new()
            .name("syncopia")
            .deploy_target(crate::cli::config_parser::DeployTarget::Cloud)
            .build()
            .expect("config");
        let mut server = sample_server_info();
        server.cloud_id = Some(9);
        server.cloud = Some("htz".to_string());

        let (cloud_id, deploy_form) = build_agent_install_deploy_request(
            &config,
            &server,
            "syncopia",
            "https://vault.try.direct",
        )
        .expect("cloud install request");

        assert_eq!(cloud_id, Some(9));
        assert_eq!(deploy_form["cloud"]["provider"], "htz");
        assert_eq!(deploy_form["server"]["server_id"], 7);
        assert_eq!(deploy_form["server"]["connection_mode"], "status_panel");
    }

    #[test]
    fn agent_install_request_includes_bootstrap_ssh_key_from_config() {
        let temp_dir = TempDir::new().expect("temp dir");
        let private_key_path = temp_dir.path().join("id_ed25519");
        let public_key_path = temp_dir.path().join("id_ed25519.pub");

        std::fs::write(&private_key_path, "TEST PRIVATE KEY").expect("private key");
        std::fs::write(&public_key_path, "ssh-ed25519 TEST PUBLIC KEY").expect("public key");

        let config = crate::cli::config_parser::ConfigBuilder::new()
            .name("syncopia")
            .deploy_target(crate::cli::config_parser::DeployTarget::Server)
            .server(crate::cli::config_parser::ServerConfig {
                host: "203.0.113.10".to_string(),
                user: "deploy".to_string(),
                ssh_key: Some(private_key_path),
                port: 2222,
            })
            .build()
            .expect("config");
        let server = sample_server_info();

        let (_, deploy_form) = build_agent_install_deploy_request(
            &config,
            &server,
            "syncopia",
            "https://vault.try.direct",
        )
        .expect("server install request");

        assert_eq!(deploy_form["server"]["ssh_private_key"], "TEST PRIVATE KEY");
        assert_eq!(
            deploy_form["server"]["public_key"],
            "ssh-ed25519 TEST PUBLIC KEY"
        );
    }

    #[test]
    fn agent_command_error_message_prefers_error_field() {
        let info = AgentCommandInfo {
            command_id: "cmd_1".to_string(),
            deployment_hash: "dep".to_string(),
            command_type: "configure_proxy".to_string(),
            status: "completed".to_string(),
            priority: "normal".to_string(),
            parameters: None,
            result: Some(serde_json::json!({
                "status": "error",
                "message": "ignored"
            })),
            error: Some(serde_json::json!(
                "Vault-backed proxy credential resolution is not configured on this agent"
            )),
            created_at: String::new(),
            updated_at: String::new(),
        };

        assert_eq!(
            agent_command_error_message(&info),
            Some(
                "Vault-backed proxy credential resolution is not configured on this agent"
                    .to_string()
            )
        );
    }

    #[test]
    fn agent_command_error_message_reads_error_result_payload() {
        let info = AgentCommandInfo {
            command_id: "cmd_2".to_string(),
            deployment_hash: "dep".to_string(),
            command_type: "configure_proxy".to_string(),
            status: "completed".to_string(),
            priority: "normal".to_string(),
            parameters: None,
            result: Some(serde_json::json!({
                "status": "error",
                "error_code": "vault_not_configured",
                "message": "Vault-backed proxy credential resolution is not configured on this agent"
            })),
            error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        assert_eq!(
            agent_command_error_message(&info),
            Some(
                "Vault-backed proxy credential resolution is not configured on this agent (vault_not_configured)"
                    .to_string()
            )
        );
    }

    #[test]
    fn agent_command_error_message_reads_structured_error_array() {
        let info = AgentCommandInfo {
            command_id: "cmd_3".to_string(),
            deployment_hash: "dep".to_string(),
            command_type: "configure_proxy".to_string(),
            status: "completed".to_string(),
            priority: "normal".to_string(),
            parameters: None,
            result: Some(serde_json::json!({
                "status": "error",
                "message": "ignored"
            })),
            error: Some(serde_json::json!({
                "errors": [{
                    "code": "npm_error",
                    "message": "NPM operation failed",
                    "details": "Failed to connect to NPM"
                }]
            })),
            created_at: String::new(),
            updated_at: String::new(),
        };

        assert_eq!(
            agent_command_error_message(&info),
            Some("NPM operation failed (npm_error): Failed to connect to NPM".to_string())
        );
    }

    #[test]
    fn agent_command_error_message_ignores_successful_results() {
        let info = AgentCommandInfo {
            command_id: "cmd_3".to_string(),
            deployment_hash: "dep".to_string(),
            command_type: "health".to_string(),
            status: "completed".to_string(),
            priority: "normal".to_string(),
            parameters: None,
            result: Some(serde_json::json!({
                "status": "ok",
                "message": "healthy"
            })),
            error: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        assert_eq!(agent_command_error_message(&info), None);
    }

    #[test]
    fn configure_proxy_no_ssl_overrides_default_ssl() {
        let command = AgentConfigureProxyCommand::new(
            "coolify".to_string(),
            "coolify.example.com".to_string(),
            8000,
            true,
            true,
            "create".to_string(),
            true,
            false,
            None,
        );

        assert!(!command.ssl);
    }

    #[test]
    fn resolve_registry_auth_for_agent_deploy_reads_env_overrides() {
        let temp_dir = TempDir::new().expect("temp dir");
        std::fs::write(
            temp_dir.path().join("stacker.yml"),
            "name: syncopia\napp:\n  type: static\ndeploy:\n  target: server\n",
        )
        .expect("write stacker.yml");

        let old_username = std::env::var("STACKER_DOCKER_USERNAME").ok();
        let old_password = std::env::var("STACKER_DOCKER_PASSWORD").ok();
        let old_registry = std::env::var("STACKER_DOCKER_REGISTRY").ok();

        std::env::set_var("STACKER_DOCKER_USERNAME", "optimum");
        std::env::set_var("STACKER_DOCKER_PASSWORD", "secret");
        std::env::set_var("STACKER_DOCKER_REGISTRY", "docker.io");

        let auth = resolve_registry_auth_for_agent_deploy(temp_dir.path()).expect("registry auth");
        assert_eq!(auth.username, "optimum");
        assert_eq!(auth.password, "secret");
        assert_eq!(auth.registry, "docker.io");

        match old_username {
            Some(value) => std::env::set_var("STACKER_DOCKER_USERNAME", value),
            None => std::env::remove_var("STACKER_DOCKER_USERNAME"),
        }
        match old_password {
            Some(value) => std::env::set_var("STACKER_DOCKER_PASSWORD", value),
            None => std::env::remove_var("STACKER_DOCKER_PASSWORD"),
        }
        match old_registry {
            Some(value) => std::env::set_var("STACKER_DOCKER_REGISTRY", value),
            None => std::env::remove_var("STACKER_DOCKER_REGISTRY"),
        }
    }
}
