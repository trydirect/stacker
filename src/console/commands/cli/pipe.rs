//! `stacker pipe` — CLI subcommands for connecting containerized apps.
//!
//! Pipe commands discover endpoints on running containers and create
//! data connections between them.
//!
//! ```text
//! CLI  ->  Stacker API (enqueue probe_endpoints)  ->  DB queue  ->  Agent probes  ->  Agent reports
//! ```

use crate::cli::error::CliError;
use crate::cli::field_matcher::{DeterministicFieldMatcher, FieldMatcher};
use crate::cli::fmt;
use crate::cli::progress;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{
    AgentCommandInfo, AgentEnqueueRequest, CreatePipeInstanceApiRequest,
    CreatePipeTemplateApiRequest, PipeInstanceInfo, PipeTemplateInfo,
};
use crate::console::commands::CallableTrait;
use crate::forms::status_panel::TriggerPipeCommandRequest;

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

    // 3. stacker.yml project → active agent (most recent heartbeat)
    let config_path = project_dir.join("stacker.yml");
    if config_path.exists() {
        if let Ok(config) = crate::cli::config_parser::StackerConfig::from_file(&config_path) {
            if let Some(ref project_name) = config.project.identity {
                let project = ctx.block_on(ctx.client.find_project_by_name(project_name))?;
                if let Some(proj) = project {
                    match ctx.block_on(ctx.client.agent_snapshot_by_project(proj.id)) {
                        Ok((_, hash)) => {
                            eprintln!(
                                "\x1b[2mℹ No --deployment specified — using active agent for project '{}': {}\x1b[0m",
                                project_name, hash
                            );
                            return Ok(hash);
                        }
                        Err(_) => {}
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
    println!(
        "Status:   {} {}",
        progress::status_icon(&info.status),
        info.status
    );

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
    pub capture_samples: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeScanCommand {
    pub fn new(
        app: String,
        protocols: Vec<String>,
        capture_samples: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            app,
            protocols,
            capture_samples,
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
            capture_samples: self.capture_samples,
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
                    if let Some(sample) = op.get("sample_response") {
                        if !sample.is_null() {
                            let sample_str = serde_json::to_string(sample).unwrap_or_default();
                            if sample_str.len() > 120 {
                                println!("           sample: {}...", &sample_str[..117]);
                            } else {
                                println!("           sample: {}", sample_str);
                            }
                        }
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
// stacker pipe create — interactive pipe creation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeCreateCommand {
    pub source: String,
    pub target: String,
    pub manual: bool,
    pub ai: bool,
    pub no_ai: bool,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeCreateCommand {
    pub fn new(
        source: String,
        target: String,
        manual: bool,
        ai: bool,
        no_ai: bool,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            source,
            target,
            manual,
            ai,
            no_ai,
            json,
            deployment,
        }
    }
}

/// Extract operations from a probe result as a flat list of
/// (method, path, summary, fields, sample_response).
fn extract_operations(
    info: &AgentCommandInfo,
) -> Vec<(
    String,
    String,
    String,
    Vec<String>,
    Option<serde_json::Value>,
)> {
    let mut ops = Vec::new();
    if let Some(ref result) = info.result {
        if let Some(endpoints) = result["endpoints"].as_array() {
            for ep in endpoints {
                let base = ep["base_url"].as_str().unwrap_or("");
                if let Some(operations) = ep["operations"].as_array() {
                    for op in operations {
                        let method = op["method"].as_str().unwrap_or("GET").to_string();
                        let path = format!("{}{}", base, op["path"].as_str().unwrap_or(""));
                        let summary = op["summary"].as_str().unwrap_or("").to_string();
                        let fields = op["fields"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let sample = op.get("sample_response").filter(|v| !v.is_null()).cloned();
                        ops.push((method, path, summary, fields, sample));
                    }
                }
            }
        }
    }
    ops
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
            capture_samples: true,
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
            capture_samples: true,
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

        if source_info.status != "completed" || target_info.status != "completed" {
            eprintln!("Scan failed for one or both apps. Cannot create pipe.");
            if source_info.status != "completed" {
                eprintln!("  Source '{}': {}", self.source, source_info.status);
            }
            if target_info.status != "completed" {
                eprintln!("  Target '{}': {}", self.target, target_info.status);
            }
            return Ok(());
        }

        // Step 2: Extract discovered endpoints
        let source_ops = extract_operations(&source_info);
        let target_ops = extract_operations(&target_info);

        if source_ops.is_empty() {
            eprintln!(
                "No endpoints discovered on source app '{}'. Cannot create pipe.",
                self.source
            );
            return Ok(());
        }
        if target_ops.is_empty() {
            eprintln!(
                "No endpoints discovered on target app '{}'. Cannot create pipe.",
                self.target
            );
            return Ok(());
        }

        // Step 3: Let user select source endpoint
        let source_labels: Vec<String> = source_ops
            .iter()
            .map(|(m, p, s, _, _)| {
                if s.is_empty() {
                    format!("{:>6} {}", m, p)
                } else {
                    format!("{:>6} {} — {}", m, p, s)
                }
            })
            .collect();

        println!("\n  Select source endpoint (data comes FROM here):");
        let source_idx = dialoguer::Select::new()
            .items(&source_labels)
            .default(0)
            .interact()?;

        let (ref src_method, ref src_path, _, ref src_fields, ref src_sample) =
            source_ops[source_idx];

        // Step 4: Let user select target endpoint
        let target_labels: Vec<String> = target_ops
            .iter()
            .map(|(m, p, s, _, _)| {
                if s.is_empty() {
                    format!("{:>6} {}", m, p)
                } else {
                    format!("{:>6} {} — {}", m, p, s)
                }
            })
            .collect();

        println!("\n  Select target endpoint (data goes TO here):");
        let target_idx = dialoguer::Select::new()
            .items(&target_labels)
            .default(0)
            .interact()?;

        let (ref tgt_method, ref tgt_path, _, ref tgt_fields, ref _tgt_sample) =
            target_ops[target_idx];

        // Step 5: Build field mapping (smart matching with sample data)
        let (field_mapping, match_result) = if !self.manual
            && !src_fields.is_empty()
            && !tgt_fields.is_empty()
        {
            let matcher = select_field_matcher(self.ai, self.no_ai);
            let result = matcher.match_fields(src_fields, tgt_fields, src_sample.as_ref());
            let mode_label = match result.mode {
                crate::cli::field_matcher::MatchingMode::Ai => "AI",
                crate::cli::field_matcher::MatchingMode::Deterministic => "deterministic",
            };
            println!(
                "\n  Auto-matching fields ({} mode, source → target):",
                mode_label
            );

            let matched: Vec<String> = result
                .mapping
                .as_object()
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            let src = v.as_str().unwrap_or("?");
                            let conf = result.confidence.get(k).copied().unwrap_or(1.0);
                            if conf < 1.0 {
                                format!("    {} ← {} (confidence: {:.0}%) ✓", k, src, conf * 100.0)
                            } else {
                                format!("    {} ← {} ✓", k, src)
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            for line in &matched {
                println!("{}", line);
            }

            // Show transformation suggestions from AI
            for suggestion in &result.suggestions {
                println!(
                    "    💡 {}: {} — {}",
                    suggestion.target_field, suggestion.expression, suggestion.description
                );
            }

            // Show unmatched target fields
            let matched_keys: Vec<&str> = result
                .mapping
                .as_object()
                .map(|m| m.keys().map(|k| k.as_str()).collect())
                .unwrap_or_default();
            let unmatched: Vec<&String> = tgt_fields
                .iter()
                .filter(|f| !matched_keys.contains(&f.as_str()))
                .collect();
            if !unmatched.is_empty() {
                println!(
                    "    Unmatched target fields: {}",
                    unmatched
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                println!("    (You can edit the field mapping later via the API)");
            }

            if matched_keys.is_empty() {
                println!("    No auto-matches found. Creating pass-through mapping.");
                let mut pass = serde_json::Map::new();
                for sf in src_fields {
                    pass.insert(sf.clone(), serde_json::Value::String(format!("$.{}", sf)));
                }
                (serde_json::Value::Object(pass), Some(result))
            } else {
                let mapping = result.mapping.clone();
                (mapping, Some(result))
            }
        } else {
            // Manual mode or no fields discovered
            println!("\n  No field auto-matching available. Creating identity mapping.");
            (serde_json::json!({}), None)
        };

        // Step 6: Ask for pipe name
        let default_name = format!("{}-to-{}", self.source, self.target);
        let pipe_name: String = dialoguer::Input::new()
            .with_prompt("Pipe name")
            .default(default_name)
            .interact_text()?;

        // Step 7: Create template via API — include matching metadata in config
        let mut config = serde_json::json!({"retry_count": 3});
        if let Some(ref result) = match_result {
            config["matching_mode"] = serde_json::Value::String(result.mode.to_string());
            if !result.confidence.is_empty() {
                let conf_map: serde_json::Map<String, serde_json::Value> = result
                    .confidence
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect();
                config["field_confidence"] = serde_json::Value::Object(conf_map);
            }
            if !result.suggestions.is_empty() {
                config["transformations"] = serde_json::json!(result
                    .suggestions
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "target": s.target_field,
                            "expr": s.expression,
                            "description": s.description,
                        })
                    })
                    .collect::<Vec<_>>());
            }
        }

        let template_request = CreatePipeTemplateApiRequest {
            name: pipe_name.clone(),
            description: Some(format!(
                "{} {} → {} {}",
                src_method, src_path, tgt_method, tgt_path
            )),
            source_app_type: self.source.clone(),
            source_endpoint: serde_json::json!({
                "path": src_path,
                "method": src_method,
            }),
            target_app_type: self.target.clone(),
            target_endpoint: serde_json::json!({
                "path": tgt_path,
                "method": tgt_method,
            }),
            target_external_url: None,
            field_mapping: field_mapping.clone(),
            config: Some(config),
            is_public: Some(false),
        };

        let pb = progress::spinner("Creating pipe template...");
        let template = ctx
            .block_on(ctx.client.create_pipe_template(&template_request))
            .map_err(|e| {
                progress::finish_error(&pb, "Template creation failed");
                e
            })?;
        progress::finish_success(&pb, "Template created");

        // Step 8: Create instance linked to this deployment
        let instance_request = CreatePipeInstanceApiRequest {
            deployment_hash: hash.clone(),
            source_container: self.source.clone(),
            target_container: Some(self.target.clone()),
            target_url: None,
            template_id: Some(template.id.clone()),
            field_mapping_override: None,
            config_override: None,
        };

        let pb = progress::spinner("Creating pipe instance...");
        let instance = ctx
            .block_on(ctx.client.create_pipe_instance(&instance_request))
            .map_err(|e| {
                progress::finish_error(&pb, "Instance creation failed");
                e
            })?;
        progress::finish_success(&pb, "Pipe instance created");

        if self.json {
            let output = serde_json::json!({
                "template": template,
                "instance": instance,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("\n  ✓ Pipe '{}' created successfully", pipe_name);
            println!("  Template ID:  {}", template.id);
            println!("  Instance ID:  {}", instance.id);
            println!("  Source:       {} ({})", self.source, src_path);
            println!("  Target:       {} ({})", self.target, tgt_path);
            println!(
                "  Status:       {} (use 'stacker pipe activate {}' to start)",
                instance.status, instance.id
            );
            println!("  Mapping:      {}", serde_json::to_string(&field_mapping)?);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe list — list active pipes for a deployment
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
        let ctx = CliRuntime::new("pipe list")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let pb = progress::spinner("Fetching pipes...");
        let pipes = ctx
            .block_on(ctx.client.list_pipe_instances(&hash))
            .map_err(|e| {
                progress::finish_error(&pb, "Failed to fetch pipes");
                e
            })?;
        progress::finish_success(&pb, &format!("{} pipe(s) found", pipes.len()));

        if pipes.is_empty() {
            println!("No pipes configured for this deployment.");
            println!("Use 'stacker pipe create <source> <target>' to create a pipe.");
            return Ok(());
        }

        if self.json {
            println!("{}", serde_json::to_string_pretty(&pipes)?);
            return Ok(());
        }

        // Table header
        println!(
            "\n{:<38} {:<15} {:<15} {:<10} {:>8} {:>8} {}",
            "ID", "SOURCE", "TARGET", "STATUS", "TRIGGERS", "ERRORS", "LAST TRIGGERED"
        );
        println!("{}", "─".repeat(120));

        for pipe in &pipes {
            let target = pipe
                .target_container
                .as_deref()
                .or(pipe.target_url.as_deref())
                .unwrap_or("-");
            let last = pipe.last_triggered_at.as_deref().unwrap_or("never");
            let status_icon = match pipe.status.as_str() {
                "active" => "● active",
                "paused" => "◉ paused",
                "error" => "✗ error",
                _ => "○ draft",
            };

            println!(
                "{:<38} {:<15} {:<15} {:<10} {:>8} {:>8} {}",
                &pipe.id,
                truncate_str(&pipe.source_container, 14),
                truncate_str(target, 14),
                status_icon,
                pipe.trigger_count,
                pipe.error_count,
                last,
            );
        }

        println!("\n{} pipe(s) total.", pipes.len());
        Ok(())
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

/// Select the appropriate field matcher based on CLI flags and stacker.yml config.
///
/// Priority:
/// 1. `--ai` flag → AI matcher (error if AI not configured)
/// 2. `--no-ai` flag → deterministic matcher
/// 3. Neither flag → check `stacker.yml` ai.enabled; if true → AI, else → deterministic
fn select_field_matcher(force_ai: bool, force_no_ai: bool) -> Box<dyn FieldMatcher> {
    if force_no_ai {
        return Box::new(DeterministicFieldMatcher);
    }

    let use_ai = if force_ai {
        true
    } else {
        // Try to read stacker.yml to check ai.enabled
        let project_dir = std::env::current_dir().unwrap_or_default();
        let config_path = project_dir.join("stacker.yml");
        if config_path.exists() {
            crate::cli::config_parser::StackerConfig::from_file(&config_path)
                .map(|c| c.ai.enabled)
                .unwrap_or(false)
        } else {
            false
        }
    };

    if use_ai {
        // Try to create AI matcher; fall back to deterministic on failure
        let project_dir = std::env::current_dir().unwrap_or_default();
        let config_path = project_dir.join("stacker.yml");
        let ai_config = config_path
            .exists()
            .then(|| {
                crate::cli::config_parser::StackerConfig::from_file(&config_path)
                    .ok()
                    .map(|c| c.ai)
            })
            .flatten();

        if let Some(config) = ai_config {
            match crate::cli::ai_field_matcher::AiFieldMatcher::new(&config) {
                Ok(matcher) => return Box::new(matcher),
                Err(e) => {
                    eprintln!(
                        "  ⚠ AI matcher unavailable ({}), falling back to deterministic",
                        e
                    );
                }
            }
        } else if force_ai {
            eprintln!(
                "  ⚠ --ai flag set but no ai: config in stacker.yml, falling back to deterministic"
            );
        }
    }

    Box::new(DeterministicFieldMatcher)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe activate — activate a pipe instance
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeActivateCommand {
    pub pipe_id: String,
    pub trigger: String,
    pub poll_interval: u32,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeActivateCommand {
    pub fn new(
        pipe_id: String,
        trigger: String,
        poll_interval: u32,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            pipe_id,
            trigger,
            poll_interval,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeActivateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe activate")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        // Fetch pipe instance details to get source/target info
        let pb = progress::spinner("Fetching pipe details...");
        let pipe = ctx
            .block_on(ctx.client.get_pipe_instance(&self.pipe_id))
            .map_err(|e| {
                progress::finish_error(&pb, "Failed");
                e
            })?
            .ok_or_else(|| {
                CliError::ConfigValidation(format!("Pipe instance '{}' not found", self.pipe_id))
            })?;
        progress::finish_success(&pb, "Pipe found");

        // Get template info for endpoint details (if linked)
        let (source_endpoint, source_method, target_endpoint, target_method, field_mapping) =
            if let Some(ref tid) = pipe.template_id {
                let templates = ctx.block_on(ctx.client.list_pipe_templates(None, None))?;
                if let Some(tmpl) = templates.iter().find(|t| &t.id == tid) {
                    (
                        tmpl.source_endpoint["path"]
                            .as_str()
                            .unwrap_or("/")
                            .to_string(),
                        tmpl.source_endpoint["method"]
                            .as_str()
                            .unwrap_or("GET")
                            .to_string(),
                        tmpl.target_endpoint["path"]
                            .as_str()
                            .unwrap_or("/")
                            .to_string(),
                        tmpl.target_endpoint["method"]
                            .as_str()
                            .unwrap_or("POST")
                            .to_string(),
                        pipe.field_mapping_override
                            .clone()
                            .unwrap_or(tmpl.field_mapping.clone()),
                    )
                } else {
                    (
                        "/".to_string(),
                        "GET".to_string(),
                        "/".to_string(),
                        "POST".to_string(),
                        serde_json::json!({}),
                    )
                }
            } else {
                (
                    "/".to_string(),
                    "GET".to_string(),
                    "/".to_string(),
                    "POST".to_string(),
                    pipe.field_mapping_override
                        .clone()
                        .unwrap_or(serde_json::json!({})),
                )
            };

        // 1. Update status to "active" via API
        let pb = progress::spinner("Setting pipe status to active...");
        ctx.block_on(ctx.client.update_pipe_status(&self.pipe_id, "active"))
            .map_err(|e| {
                progress::finish_error(&pb, "Status update failed");
                e
            })?;
        progress::finish_success(&pb, "Status: active");

        // 2. Send activate_pipe command to agent
        let params = serde_json::json!({
            "pipe_instance_id": self.pipe_id,
            "source_container": pipe.source_container,
            "source_endpoint": source_endpoint,
            "source_method": source_method,
            "target_container": pipe.target_container,
            "target_url": pipe.target_url,
            "target_endpoint": target_endpoint,
            "target_method": target_method,
            "field_mapping": field_mapping,
            "trigger_type": self.trigger,
            "poll_interval_secs": self.poll_interval,
        });

        let request = AgentEnqueueRequest::new(&hash, "activate_pipe").with_raw_parameters(params);

        let info = run_agent_command(
            &ctx,
            &request,
            "Activating pipe on agent",
            PROBE_TIMEOUT_SECS,
        )?;

        print_command_result(&info, self.json);

        if !self.json && info.status == "completed" {
            println!("\n  ✓ Pipe '{}' is now active", self.pipe_id);
            println!("  Trigger type: {}", self.trigger);
            if self.trigger == "poll" {
                println!("  Poll interval: {}s", self.poll_interval);
            }
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe deactivate — stop a pipe
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeDeactivateCommand {
    pub pipe_id: String,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeDeactivateCommand {
    pub fn new(pipe_id: String, json: bool, deployment: Option<String>) -> Self {
        Self {
            pipe_id,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeDeactivateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe deactivate")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        // 1. Update status to "paused" via API
        let pb = progress::spinner("Setting pipe status to paused...");
        ctx.block_on(ctx.client.update_pipe_status(&self.pipe_id, "paused"))
            .map_err(|e| {
                progress::finish_error(&pb, "Status update failed");
                e
            })?;
        progress::finish_success(&pb, "Status: paused");

        // 2. Send deactivate_pipe command to agent
        let params = serde_json::json!({
            "pipe_instance_id": self.pipe_id,
        });

        let request =
            AgentEnqueueRequest::new(&hash, "deactivate_pipe").with_raw_parameters(params);

        let info = run_agent_command(
            &ctx,
            &request,
            "Deactivating pipe on agent",
            PROBE_TIMEOUT_SECS,
        )?;

        print_command_result(&info, self.json);

        if !self.json && info.status == "completed" {
            println!("\n  ✓ Pipe '{}' deactivated", self.pipe_id);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe trigger — one-shot pipe execution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeTriggerCommand {
    pub pipe_id: String,
    pub data: Option<String>,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeTriggerCommand {
    pub fn new(
        pipe_id: String,
        data: Option<String>,
        json: bool,
        deployment: Option<String>,
    ) -> Self {
        Self {
            pipe_id,
            data,
            json,
            deployment,
        }
    }
}

fn build_trigger_pipe_request(
    pipe: &PipeInstanceInfo,
    template: Option<&PipeTemplateInfo>,
    input_data: Option<serde_json::Value>,
) -> TriggerPipeCommandRequest {
    let (target_endpoint, target_method, field_mapping) = if let Some(tmpl) = template {
        (
            tmpl.target_endpoint["path"]
                .as_str()
                .unwrap_or("/")
                .to_string(),
            tmpl.target_endpoint["method"]
                .as_str()
                .unwrap_or("POST")
                .to_string(),
            pipe.field_mapping_override
                .clone()
                .unwrap_or(tmpl.field_mapping.clone()),
        )
    } else {
        (
            "/".to_string(),
            "POST".to_string(),
            pipe.field_mapping_override
                .clone()
                .unwrap_or(serde_json::json!({})),
        )
    };

    TriggerPipeCommandRequest::new_manual(
        pipe.id.clone(),
        input_data,
        pipe.target_url.clone(),
        target_endpoint,
        target_method,
        Some(field_mapping),
        "manual".to_string(),
    )
}

impl CallableTrait for PipeTriggerCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe trigger")?;
        let hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let input_data = match &self.data {
            Some(raw) => {
                let parsed: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|e| CliError::ConfigValidation(format!("Invalid JSON data: {}", e)))?;
                Some(parsed)
            }
            None => None,
        };

        let pb = progress::spinner("Fetching pipe details...");
        let pipe = ctx
            .block_on(ctx.client.get_pipe_instance(&self.pipe_id))
            .map_err(|e| {
                progress::finish_error(&pb, "Failed");
                e
            })?
            .ok_or_else(|| {
                CliError::ConfigValidation(format!("Pipe instance '{}' not found", self.pipe_id))
            })?;
        progress::finish_success(&pb, "Pipe found");

        let template = if let Some(ref tid) = pipe.template_id {
            let templates = ctx.block_on(ctx.client.list_pipe_templates(None, None))?;
            templates.into_iter().find(|tmpl| &tmpl.id == tid)
        } else {
            None
        };

        let params = serde_json::to_value(build_trigger_pipe_request(
            &pipe,
            template.as_ref(),
            input_data,
        ))?;

        let request = AgentEnqueueRequest::new(&hash, "trigger_pipe").with_raw_parameters(params);

        let info = run_agent_command(&ctx, &request, "Triggering pipe", PROBE_TIMEOUT_SECS)?;

        print_command_result(&info, self.json);

        if !self.json {
            if info.status == "completed" {
                if let Some(ref result) = info.result {
                    let success = result["success"].as_bool().unwrap_or(false);
                    if success {
                        println!("\n  ✓ Pipe '{}' triggered successfully", self.pipe_id);
                    } else {
                        let error = result["error"].as_str().unwrap_or("unknown error");
                        eprintln!("\n  ✗ Pipe trigger failed: {}", error);
                    }
                }
            }
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe history — show execution history
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeHistoryCommand {
    pub instance_id: String,
    pub limit: i64,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeHistoryCommand {
    pub fn new(instance_id: String, limit: i64, json: bool, deployment: Option<String>) -> Self {
        Self {
            instance_id,
            limit,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeHistoryCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe history")?;
        let _hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let pb = progress::spinner("Fetching execution history...");
        let executions = ctx
            .block_on(
                ctx.client
                    .list_pipe_executions(&self.instance_id, self.limit, 0),
            )
            .map_err(|e| {
                progress::finish_error(&pb, "Failed to fetch history");
                e
            })?;
        progress::finish_success(&pb, &format!("{} execution(s) found", executions.len()));

        if executions.is_empty() {
            println!(
                "No executions recorded for pipe instance '{}'.",
                self.instance_id
            );
            println!(
                "Use 'stacker pipe trigger {}' to execute the pipe.",
                self.instance_id
            );
            return Ok(());
        }

        if self.json {
            println!("{}", serde_json::to_string_pretty(&executions)?);
            return Ok(());
        }

        println!(
            "\n{:<38} {:<10} {:<10} {:>10} {:<22} {}",
            "EXECUTION ID", "TRIGGER", "STATUS", "DURATION", "STARTED", "ERROR"
        );
        println!("{}", "─".repeat(110));

        for exec in &executions {
            let status_icon = match exec.status.as_str() {
                "success" => "✓ success",
                "failed" => "✗ failed",
                "running" => "⟳ running",
                _ => &exec.status,
            };
            let duration = exec
                .duration_ms
                .map(|ms| format!("{}ms", ms))
                .unwrap_or_else(|| "-".to_string());
            let error = exec.error.as_deref().unwrap_or("");

            println!(
                "{:<38} {:<10} {:<10} {:>10} {:<22} {}",
                &exec.id,
                truncate_str(&exec.trigger_type, 9),
                status_icon,
                duration,
                truncate_str(&exec.started_at, 21),
                truncate_str(error, 30),
            );
        }

        println!("\n{} execution(s) shown.", executions.len());
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// stacker pipe replay — replay a previous execution
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct PipeReplayCommand {
    pub execution_id: String,
    pub json: bool,
    pub deployment: Option<String>,
}

impl PipeReplayCommand {
    pub fn new(execution_id: String, json: bool, deployment: Option<String>) -> Self {
        Self {
            execution_id,
            json,
            deployment,
        }
    }
}

impl CallableTrait for PipeReplayCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("pipe replay")?;
        let _hash = resolve_deployment_hash(&self.deployment, &ctx)?;

        let pb = progress::spinner(&format!("Replaying execution {}...", &self.execution_id));
        let replay = ctx
            .block_on(ctx.client.replay_pipe_execution(&self.execution_id))
            .map_err(|e| {
                progress::finish_error(&pb, "Replay failed");
                e
            })?;
        progress::finish_success(&pb, "Replay initiated");

        if self.json {
            println!("{}", serde_json::to_string_pretty(&replay)?);
            return Ok(());
        }

        println!("\n  Replay execution: {}", replay.execution_id);
        println!("  Replaying from:   {}", replay.replay_of);
        if let Some(ref cmd_id) = replay.command_id {
            println!("  Command ID:       {}", cmd_id);
            println!("\n  Replay enqueued. Use 'stacker pipe history' to check results.");
        } else {
            println!("  Status:           {}", replay.status);
            println!("  (command not enqueued — check server logs)");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::field_matcher::{DeterministicFieldMatcher, FieldMatcher};
    use serde_json::json;

    #[test]
    fn test_smart_field_match_exact() {
        let matcher = DeterministicFieldMatcher;
        let src = vec!["email".to_string(), "name".to_string(), "id".to_string()];
        let tgt = vec!["email".to_string(), "name".to_string()];
        let result = matcher.match_fields(&src, &tgt, None);
        let map = result.mapping.as_object().unwrap();
        assert_eq!(map["email"], "$.email");
        assert_eq!(map["name"], "$.name");
    }

    #[test]
    fn test_smart_field_match_case_insensitive() {
        let matcher = DeterministicFieldMatcher;
        let src = vec!["Email".to_string(), "UserName".to_string()];
        let tgt = vec!["email".to_string(), "username".to_string()];
        let result = matcher.match_fields(&src, &tgt, None);
        let map = result.mapping.as_object().unwrap();
        assert_eq!(map["email"], "$.Email");
        assert_eq!(map["username"], "$.UserName");
    }

    #[test]
    fn test_smart_field_match_semantic_aliases() {
        let matcher = DeterministicFieldMatcher;
        let src = vec!["user_email".to_string(), "display_name".to_string()];
        let tgt = vec!["email".to_string(), "name".to_string()];
        let result = matcher.match_fields(&src, &tgt, None);
        let map = result.mapping.as_object().unwrap();
        assert_eq!(map["email"], "$.user_email");
        assert_eq!(map["name"], "$.display_name");
    }

    #[test]
    fn test_smart_field_match_type_aware_suffix() {
        let matcher = DeterministicFieldMatcher;
        let src = vec!["author_id".to_string(), "post_id".to_string()];
        let tgt = vec!["user_id".to_string()];
        let sample = json!({"author_id": 42, "post_id": 1});
        let result = matcher.match_fields(&src, &tgt, Some(&sample));
        let map = result.mapping.as_object().unwrap();
        assert_eq!(map["user_id"], "$.author_id");
    }

    #[test]
    fn test_smart_field_match_no_matches() {
        let matcher = DeterministicFieldMatcher;
        let src = vec!["foo".to_string()];
        let tgt = vec!["bar".to_string()];
        let result = matcher.match_fields(&src, &tgt, None);
        let map = result.mapping.as_object().unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_smart_field_match_mixed_strategies() {
        let matcher = DeterministicFieldMatcher;
        let src = vec![
            "email".to_string(),
            "display_name".to_string(),
            "Phone".to_string(),
        ];
        let tgt = vec![
            "email".to_string(),
            "name".to_string(),
            "phone".to_string(),
            "unknown".to_string(),
        ];
        let result = matcher.match_fields(&src, &tgt, None);
        let map = result.mapping.as_object().unwrap();
        assert_eq!(map.len(), 3);
        assert_eq!(map["email"], "$.email");
        assert_eq!(map["name"], "$.display_name");
        assert_eq!(map["phone"], "$.Phone");
        assert!(!map.contains_key("unknown"));
    }

    #[test]
    fn build_trigger_pipe_request_uses_external_target_details() {
        let pipe = PipeInstanceInfo {
            id: "pipe-123".to_string(),
            template_id: Some("tmpl-123".to_string()),
            deployment_hash: "dep-123".to_string(),
            source_container: "source".to_string(),
            target_container: None,
            target_url: Some("https://hooks.example.com".to_string()),
            field_mapping_override: Some(json!({ "email": "$.user.email" })),
            config_override: None,
            status: "active".to_string(),
            last_triggered_at: None,
            trigger_count: 0,
            error_count: 0,
            created_by: "test-user".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let template = PipeTemplateInfo {
            id: "tmpl-123".to_string(),
            name: "crm-webhook".to_string(),
            description: None,
            source_app_type: "source".to_string(),
            source_endpoint: json!({ "path": "/source", "method": "GET" }),
            target_app_type: "target".to_string(),
            target_endpoint: json!({ "path": "/webhook/pipe", "method": "POST" }),
            target_external_url: Some("https://hooks.example.com".to_string()),
            field_mapping: json!({ "ignored": "$.ignored" }),
            config: None,
            is_public: Some(false),
            created_by: "test-user".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let request = build_trigger_pipe_request(
            &pipe,
            Some(&template),
            Some(json!({ "user": { "email": "dev@try.direct" } })),
        );

        assert_eq!(request.pipe_instance_id, "pipe-123");
        assert_eq!(
            request.target_url.as_deref(),
            Some("https://hooks.example.com")
        );
        assert_eq!(request.target_endpoint, "/webhook/pipe");
        assert_eq!(request.target_method, "POST");
        assert_eq!(
            request.field_mapping,
            Some(json!({ "email": "$.user.email" }))
        );
        assert_eq!(request.trigger_type, "manual");
    }
}
