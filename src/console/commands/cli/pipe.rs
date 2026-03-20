//! `stacker pipe` — CLI subcommands for connecting containerized apps.
//!
//! Pipe commands discover endpoints on running containers and create
//! data connections between them.
//!
//! ```text
//! CLI  ->  Stacker API (enqueue probe_endpoints)  ->  DB queue  ->  Agent probes  ->  Agent reports
//! ```

use crate::cli::error::CliError;
use crate::cli::fmt;
use crate::cli::progress;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{AgentCommandInfo, AgentEnqueueRequest};
use crate::console::commands::CallableTrait;

/// Default poll timeout for pipe probe commands (seconds).
const PROBE_TIMEOUT_SECS: u64 = 90;

/// Default poll interval (seconds).
const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Deployment hash resolution (mirrors agent module)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Resolve a deployment hash from explicit flag, deployment lock, or stacker.yml project name.
///
/// Resolution order:
/// 1. Explicit `--deployment` flag value
/// 2. `.stacker/deployment.lock` -> `deployment_id` -> API lookup for hash
/// 3. `stacker.yml` project name -> API project lookup -> latest deployment hash
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

    // 3. stacker.yml project name -> API lookup
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
            progress::finish_success(&pb, &format!("{} done", spinner_msg));
        }
        Ok(info) => {
            progress::finish_error(&pb, &format!("{} -- {}", spinner_msg, info.status));
        }
        Err(e) => {
            progress::finish_error(&pb, &format!("{} -- {}", spinner_msg, e));
        }
    }

    result
}

fn print_command_result(info: &AgentCommandInfo, json_output: bool) {
    if json_output {
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
// stacker pipe scan
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeScanCommand {
    pub app: String,
    pub protocols: Vec<String>,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeScanCommand {
    pub fn new(
        app: String,
        protocols: Vec<String>,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            app,
            protocols,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeScanCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe scan")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let protocols = if self.protocols.is_empty() {
            vec!["openapi".to_string(), "rest".to_string()]
        } else {
            self.protocols.clone()
        };

        let params = crate::forms::status_panel::ProbeEndpointsCommandRequest {
            app_code: self.app.clone(),
            container: None,
            protocols,
            probe_timeout: 5,
        };

        let request = AgentEnqueueRequest::new(&hash, "probe_endpoints")
            .with_parameters(&params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let info = run_agent_command(
            &ctx,
            &request,
            &format!("Scanning {} for endpoints", self.app),
            PROBE_TIMEOUT_SECS,
        )?;

        if self.json {
            print_command_result(&info, true);
        } else {
            print_scan_result(&info);
        }

        Ok(())
    }
}

fn print_scan_result(info: &AgentCommandInfo) {
    if info.status != "completed" {
        if let Some(ref error) = info.error {
            eprintln!("Scan failed: {}", fmt::pretty_json(error));
        } else {
            eprintln!("Scan failed: unknown error");
        }
        return;
    }

    let result = match &info.result {
        Some(r) => r,
        None => {
            eprintln!("No scan results returned");
            return;
        }
    };

    let app_code = result["app_code"].as_str().unwrap_or("unknown");
    let protocols = result["protocols_detected"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    println!("\n  App: {}", app_code);
    println!(
        "  Protocols detected: {}",
        if protocols.is_empty() {
            "none"
        } else {
            &protocols
        }
    );

    if let Some(endpoints) = result["endpoints"].as_array() {
        for ep in endpoints {
            let protocol = ep["protocol"].as_str().unwrap_or("unknown");
            let base_url = ep["base_url"].as_str().unwrap_or("");
            let spec_url = ep["spec_url"].as_str().unwrap_or("");
            println!("\n  [{protocol}] {base_url}{spec_url}");

            if let Some(operations) = ep["operations"].as_array() {
                for op in operations {
                    let method = op["method"].as_str().unwrap_or("?");
                    let path = op["path"].as_str().unwrap_or("?");
                    let summary = op["summary"].as_str().unwrap_or("");
                    let fields = op["fields"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();

                    print!("    {:>6} {}", method, path);
                    if !summary.is_empty() {
                        print!("  -- {}", summary);
                    }
                    println!();
                    if !fields.is_empty() {
                        println!("           fields: [{}]", fields);
                    }
                }
            }
        }
    }

    if let Some(forms) = result["forms"].as_array() {
        if !forms.is_empty() {
            println!("\n  HTML Forms:");
            for form in forms {
                let id = form["id"].as_str().unwrap_or("?");
                let action = form["action"].as_str().unwrap_or("?");
                let method = form["method"].as_str().unwrap_or("?");
                let fields = form["fields"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();

                println!("    #{}  {} {}", id, method, action);
                if !fields.is_empty() {
                    println!("      fields: [{}]", fields);
                }
            }
        }
    }

    println!();
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe create (placeholder for Phase 1)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeCreateCommand {
    pub source: String,
    pub target: String,
    pub manual: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeCreateCommand {
    pub fn new(
        source: String,
        target: String,
        manual: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            source,
            target,
            manual,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeCreateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe create")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        // Step 1: Scan both source and target
        println!(
            "Scanning source app '{}' and target app '{}'...",
            self.source, self.target
        );

        let source_params = crate::forms::status_panel::ProbeEndpointsCommandRequest {
            app_code: self.source.clone(),
            container: None,
            protocols: vec![
                "openapi".to_string(),
                "html_forms".to_string(),
                "rest".to_string(),
            ],
            probe_timeout: 5,
        };

        let source_request = AgentEnqueueRequest::new(&hash, "probe_endpoints")
            .with_parameters(&source_params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let source_info = run_agent_command(
            &ctx,
            &source_request,
            &format!("Scanning source: {}", self.source),
            PROBE_TIMEOUT_SECS,
        )?;

        let target_params = crate::forms::status_panel::ProbeEndpointsCommandRequest {
            app_code: self.target.clone(),
            container: None,
            protocols: vec!["openapi".to_string(), "rest".to_string()],
            probe_timeout: 5,
        };

        let target_request = AgentEnqueueRequest::new(&hash, "probe_endpoints")
            .with_parameters(&target_params)
            .map_err(|e| CliError::ConfigValidation(format!("Invalid parameters: {}", e)))?;

        let target_info = run_agent_command(
            &ctx,
            &target_request,
            &format!("Scanning target: {}", self.target),
            PROBE_TIMEOUT_SECS,
        )?;

        // Print results for both
        println!("\n=== Source: {} ===", self.source);
        print_scan_result(&source_info);

        println!("\n=== Target: {} ===", self.target);
        print_scan_result(&target_info);

        // TODO Phase 1: Interactive matching + AI field mapping + pipe storage
        println!("Interactive pipe creation will be available in the next release.");
        println!("For now, use 'stacker pipe scan <app>' to discover endpoints.");

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe list (placeholder for Phase 1)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeListCommand {
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeListCommand {
    pub fn new(json: bool, deployment: Option<String>) -> Self {
        Self { json, deployment }
    }
}

impl CallableTrait for PipeListCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO Phase 1: Query pipes from database
        println!("No pipes configured yet.");
        println!("Use 'stacker pipe create <source> <target>' to create a pipe.");
        Ok(())
    }
}
