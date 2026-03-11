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

use crate::cli::error::CliError;
use crate::cli::fmt;
use crate::cli::progress;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{AgentCommandInfo, AgentEnqueueRequest};
use crate::console::commands::CallableTrait;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Deployment hash resolution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Default poll timeout for agent commands (seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Default poll interval (seconds).
const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;

/// Resolve a deployment hash from explicit flag, deployment lock, or stacker.yml project name.
///
/// Resolution order:
/// 1. Explicit `--deployment` flag value
/// 2. `.stacker/deployment.lock` → `deployment_id` → API lookup for hash
/// 3. `stacker.yml` project name → API project lookup → latest deployment hash
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

    // 2. Deployment lock
    if let Some(lock) = crate::cli::deployment_lock::DeploymentLock::load(&project_dir)? {
        if let Some(dep_id) = lock.deployment_id {
            let info = ctx.block_on(ctx.client.get_deployment_status(dep_id as i32))?;
            if let Some(info) = info {
                return Ok(info.deployment_hash);
            }
        }
    }

    // 3. stacker.yml project name → API lookup
    let config_path = project_dir.join("stacker.yml");
    if config_path.exists() {
        if let Ok(config) = crate::cli::config_parser::StackerConfig::from_file(&config_path) {
            if let Some(ref project_name) = config.project.identity {
                let project = ctx.block_on(ctx.client.find_project_by_name(project_name))?;
                if let Some(proj) = project {
                    let dep = ctx.block_on(ctx.client.get_deployment_status_by_project(proj.id))?;
                    if let Some(dep) = dep {
                        return Ok(dep.deployment_hash);
                    }
                }
            }
        }
    }

    Err(CliError::ConfigValidation(
        "Cannot determine deployment hash.\n\
         Use --deployment <HASH>, or run from a directory with a deployment lock or stacker.yml."
            .to_string(),
    ))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Shared agent command execution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Execute an agent command with spinner and polling.
///
/// 1. Enqueues the command via the Stacker API
/// 2. Shows a spinner while polling for the result
/// 3. Returns the completed `AgentCommandInfo`
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

        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
        let interval = std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS);

        loop {
            tokio::time::sleep(interval).await;

            if tokio::time::Instant::now() >= deadline {
                return Err(CliError::AgentCommandTimeout {
                    command_id: command_id.clone(),
                });
            }

            let status = ctx
                .client
                .agent_command_status(&deployment_hash, &command_id)
                .await?;

            progress::update_message(
                &pb,
                &format!("{} [{}]", spinner_msg, status.status),
            );

            match status.status.as_str() {
                "completed" | "failed" => return Ok(status),
                _ => continue,
            }
        }
    });

    match &result {
        Ok(info) if info.status == "completed" => {
            progress::finish_success(&pb, &format!("{} ✓", spinner_msg));
        }
        Ok(info) => {
            progress::finish_error(&pb, &format!("{} — {}", spinner_msg, info.status));
        }
        Err(e) => {
            progress::finish_error(&pb, &format!("{} — {}", spinner_msg, e));
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
    println!("Status:   {} {}", progress::status_icon(&info.status), info.status);

    if let Some(ref result) = info.result {
        println!("\n{}", fmt::pretty_json(result));
    }

    if let Some(ref error) = info.error {
        eprintln!("\nError: {}", fmt::pretty_json(error));
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
        Self { app_code, json, deployment, include_system }
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

/// `stacker agent logs <app> [--limit N] [--json] [--deployment <hash>]`
pub struct AgentLogsCommand {
    pub app_code: String,
    pub limit: Option<i32>,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentLogsCommand {
    pub fn new(
        app_code: String,
        limit: Option<i32>,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { app_code, limit, json, deployment }
    }
}

impl CallableTrait for AgentLogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent logs")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let params = crate::forms::status_panel::LogsCommandRequest {
            app_code: self.app_code.clone(),
            container: None,
            cursor: None,
            limit: self.limit.unwrap_or(400),
            streams: vec!["stdout".to_string(), "stderr".to_string()],
            redact: true,
        };

        let request = AgentEnqueueRequest::new(&hash, "logs")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(&ctx, &request, "Fetching logs", DEFAULT_TIMEOUT_SECS)?;
        print_command_result(&info, self.json);
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
    pub fn new(
        app_code: String,
        force: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { app_code, force, json, deployment }
    }
}

impl CallableTrait for AgentRestartCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent restart")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

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

/// `stacker agent deploy-app <app> [--image <img>] [--force] [--json] [--deployment <hash>]`
pub struct AgentDeployAppCommand {
    pub app_code: String,
    pub image: Option<String>,
    pub force_recreate: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentDeployAppCommand {
    pub fn new(
        app_code: String,
        image: Option<String>,
        force_recreate: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { app_code, image, force_recreate, json, deployment }
    }
}

impl CallableTrait for AgentDeployAppCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent deploy-app")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let params = crate::forms::status_panel::DeployAppCommandRequest {
            app_code: self.app_code.clone(),
            compose_content: None,
            image: self.image.clone(),
            env_vars: None,
            pull: true,
            force_recreate: self.force_recreate,
        };

        let request = AgentEnqueueRequest::new(&hash, "deploy_app")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?
            .with_timeout(300);

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Deploying {}", self.app_code),
            300,
        )?;
        print_command_result(&info, self.json);
        Ok(())
    }
}

// ── Remove App ───────────────────────────────────────

/// `stacker agent remove-app <app> [--volumes] [--json] [--deployment <hash>]`
pub struct AgentRemoveAppCommand {
    pub app_code: String,
    pub remove_volumes: bool,
    pub remove_image: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentRemoveAppCommand {
    pub fn new(
        app_code: String,
        remove_volumes: bool,
        remove_image: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { app_code, remove_volumes, remove_image, json, deployment }
    }
}

impl CallableTrait for AgentRemoveAppCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent remove-app")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

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

/// `stacker agent configure-firewall [--action add] [--public-ports 80/tcp,443/tcp] [--private-ports 5432/tcp:10.0.0.0/8] [--json] [--deployment <hash>]`
pub struct AgentConfigureFirewallCommand {
    pub action: String,
    pub app_code: Option<String>,
    pub public_ports: Vec<String>,
    pub private_ports: Vec<String>,
    pub persist: bool,
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
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { action, app_code, public_ports, private_ports, persist, json, deployment }
    }

    /// Parse "80/tcp" or "443" into a FirewallPortRule (source defaults to 0.0.0.0/0).
    fn parse_public_port(s: &str) -> Result<crate::forms::status_panel::FirewallPortRule, String> {
        let (port, protocol) = Self::parse_port_proto(s)?;
        Ok(crate::forms::status_panel::FirewallPortRule {
            port,
            protocol,
            source: "0.0.0.0/0".to_string(),
            comment: None,
        })
    }

    /// Parse "5432/tcp:10.0.0.0/8" or "5432:10.0.0.0/8" into a FirewallPortRule.
    fn parse_private_port(s: &str) -> Result<crate::forms::status_panel::FirewallPortRule, String> {
        // Split on first ':' that separates port/proto from source
        // Format: port[/proto]:source
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 || parts[1].is_empty() {
            return Err(format!(
                "Invalid private port '{}'. Expected format: port[/proto]:source (e.g. 5432/tcp:10.0.0.0/8)",
                s
            ));
        }
        let (port, protocol) = Self::parse_port_proto(parts[0])?;
        Ok(crate::forms::status_panel::FirewallPortRule {
            port,
            protocol,
            source: parts[1].to_string(),
            comment: None,
        })
    }

    fn parse_port_proto(s: &str) -> Result<(u16, String), String> {
        if let Some((port_s, proto)) = s.split_once('/') {
            let port: u16 = port_s.parse().map_err(|_| format!("Invalid port number: {}", port_s))?;
            Ok((port, proto.to_string()))
        } else {
            let port: u16 = s.parse().map_err(|_| format!("Invalid port number: {}", s))?;
            Ok((port, "tcp".to_string()))
        }
    }
}

impl CallableTrait for AgentConfigureFirewallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent configure-firewall")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

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

/// `stacker agent configure-proxy <app> --domain <d> --port <p> [--json] [--deployment <hash>]`
pub struct AgentConfigureProxyCommand {
    pub app_code: String,
    pub domain: String,
    pub port: u16,
    pub ssl: bool,
    pub action: String,
    pub json: bool,
    pub deployment: Option<String>,
}

impl AgentConfigureProxyCommand {
    pub fn new(
        app_code: String,
        domain: String,
        port: u16,
        ssl: bool,
        action: String,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self { app_code, domain, port, ssl, action, json, deployment }
    }
}

impl CallableTrait for AgentConfigureProxyCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("agent configure-proxy")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

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
                progress::finish_success(&pb, "Agent status fetched");

                if self.json {
                    println!("{}", fmt::pretty_json(&snap));
                } else {
                    print_snapshot_summary(&snap);
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

/// Pretty-print a snapshot summary for human consumption.
fn print_snapshot_summary(snap: &serde_json::Value) {
    println!("{}", fmt::separator(60));

    // Agent info
    if let Some(agent) = snap.get("agent") {
        let status = agent
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let version = agent
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
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

    // Containers
    if let Some(containers) = snap.get("containers").and_then(|v| v.as_array()) {
        if containers.is_empty() {
            println!("Containers: none");
        } else {
            println!(
                "{:<20} {:<12} {:<30}",
                "CONTAINER", "STATE", "IMAGE"
            );
            for c in containers {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                let state = c.get("state").and_then(|v| v.as_str()).unwrap_or("-");
                let image = c.get("image").and_then(|v| v.as_str()).unwrap_or("-");
                println!(
                    "{:<20} {} {:<10} {:<30}",
                    fmt::truncate(name, 18),
                    progress::status_icon(state),
                    state,
                    fmt::truncate(image, 28),
                );
            }
        }
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
        Self { command_type, params, timeout, json, deployment }
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

impl CallableTrait for AgentInstallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::cli::stacker_client::{self, DEFAULT_VAULT_URL};

        let project_dir = std::env::current_dir().map_err(CliError::Io)?;
        let config_path = match &self.file {
            Some(f) => project_dir.join(f),
            None => project_dir.join("stacker.yml"),
        };

        let config = crate::cli::config_parser::StackerConfig::from_file(&config_path)?;

        let project_name = config
            .project
            .identity
            .clone()
            .unwrap_or_else(|| config.name.clone());

        let ctx = CliRuntime::new("agent install")?;
        let pb = progress::spinner("Installing Status Panel agent");

        let result: Result<stacker_client::DeployResponse, CliError> = ctx.block_on(async {
            // 1. Find the project
            progress::update_message(&pb, "Finding project...");
            let project = ctx
                .client
                .find_project_by_name(&project_name)
                .await?
                .ok_or_else(|| CliError::ConfigValidation(format!(
                    "Project '{}' not found on the Stacker server.\n\
                     Deploy the project first with: stacker deploy --target cloud",
                    project_name
                )))?;

            // 2. Find the server for this project
            progress::update_message(&pb, "Finding server...");
            let servers = ctx.client.list_servers().await?;
            let server = servers
                .into_iter()
                .find(|s| s.project_id == project.id)
                .ok_or_else(|| CliError::ConfigValidation(format!(
                    "No server found for project '{}' (id={}).\n\
                     Deploy the project first with: stacker deploy --target cloud",
                    project_name, project.id
                )))?;

            let cloud_id = server.cloud_id.ok_or_else(|| CliError::ConfigValidation(
                "Server has no associated cloud credentials.\n\
                 Cannot install Status Panel without cloud credentials."
                    .to_string(),
            ))?;

            // 3. Build a minimal deploy form with only the statuspanel feature
            progress::update_message(&pb, "Preparing deploy payload...");
            let vault_url = std::env::var("STACKER_VAULT_URL")
                .unwrap_or_else(|_| DEFAULT_VAULT_URL.to_string());

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

            // 4. Trigger the deploy
            progress::update_message(&pb, "Deploying Status Panel...");
            let resp = ctx.client.deploy(project.id, Some(cloud_id), deploy_form).await?;
            Ok(resp)
        });

        match result {
            Ok(resp) => {
                progress::finish_success(&pb, "Status Panel agent installation triggered");

                if self.json {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap_or_default());
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
        print_snapshot_summary(&snap);
    }
}
