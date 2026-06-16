use crate::cli::config_parser::StackerConfig;
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{build_deploy_form, StackerClient};
use crate::console::commands::CallableTrait;
use serde_json::{Map, Value};
use std::path::PathBuf;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace find/install
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct MarketplaceFindCommand {
    query: String,
    json: bool,
    limit: Option<u32>,
}

impl MarketplaceFindCommand {
    pub fn new(query: String, json: bool, limit: Option<u32>) -> Self {
        Self { query, json, limit }
    }
}

impl CallableTrait for MarketplaceFindCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ctx = CliRuntime::new("find marketplace templates")?;
        let templates = ctx.block_on(async {
            ctx.client
                .search_marketplace_templates(
                    Some(&self.query),
                    None,
                    None,
                    Some(self.limit.unwrap_or(20)),
                )
                .await
        })?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&templates)?);
            return Ok(());
        }

        if templates.is_empty() {
            println!("No marketplace templates found for '{}'.", self.query);
            return Ok(());
        }

        println!(
            "{:<24} {:<28} {:<14} {}",
            "ITEM", "NAME", "PLAN", "DESCRIPTION"
        );
        println!("{}", "\u{2500}".repeat(92));
        for template in &templates {
            println!(
                "{:<24} {:<28} {:<14} {}",
                truncate(&template.slug, 22),
                truncate(&template.name, 26),
                template.required_plan_name.as_deref().unwrap_or("pro/team"),
                truncate(template.description.as_deref().unwrap_or(""), 26),
            );
        }
        eprintln!(
            "\nInstall one with: stacker install {}",
            templates
                .first()
                .map(|template| template.slug.as_str())
                .unwrap_or("<item>")
        );

        Ok(())
    }
}

pub struct MarketplaceInstallCommand {
    template: String,
    name: Option<String>,
    file: PathBuf,
    force: bool,
    json: bool,
    domain: Option<String>,
    set_values: Vec<String>,
}

impl MarketplaceInstallCommand {
    pub fn new(
        template: String,
        name: Option<String>,
        file: Option<PathBuf>,
        force: bool,
        json: bool,
        domain: Option<String>,
        set_values: Vec<String>,
    ) -> Self {
        Self {
            template,
            name,
            file: file.unwrap_or_else(|| PathBuf::from("stacker.yml")),
            force,
            json,
            domain,
            set_values,
        }
    }
}

impl CallableTrait for MarketplaceInstallCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (mut deploy_form, config_inputs) = if self.file.exists() {
            let config = StackerConfig::from_file(&self.file)?;
            let mut form = build_deploy_form(&config);
            if let Some(stack) = form
                .get_mut("stack")
                .and_then(|value| value.as_object_mut())
            {
                stack.insert(
                    "stack_code".to_string(),
                    serde_json::Value::String(self.template.clone()),
                );
            }
            (Some(form), config.install.inputs)
        } else {
            (None, Default::default())
        };
        let install_inputs =
            resolve_install_inputs(config_inputs, self.domain.as_deref(), &self.set_values)?;
        if let Some(form) = deploy_form.as_mut() {
            apply_install_inputs_to_deploy_form(form, &install_inputs);
        }
        let install_inputs = if install_inputs.is_empty() {
            None
        } else {
            Some(install_inputs)
        };

        let ctx = CliRuntime::new("install marketplace template")?;
        let response = ctx.block_on(async {
            ctx.client
                .install_marketplace_template(
                    &self.template,
                    self.name.as_deref(),
                    deploy_form,
                    install_inputs,
                )
                .await
        })?;

        if self.json {
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(());
        }

        if let Some(deployment_id) = response.deployment_id {
            println!(
                "Installed '{}' as project #{} and started deployment #{}.",
                response.template.slug, response.project.id, deployment_id
            );
            println!("Track with: stacker deployments state {}", deployment_id);
            return Ok(());
        }

        let stack_definition = response
            .latest_version
            .get("stack_definition")
            .cloned()
            .ok_or_else(|| {
                CliError::ConfigValidation(
                    "Install response did not include a stack definition".to_string(),
                )
            })?;
        let yaml = serde_yaml::to_string(&stack_definition).map_err(|err| {
            CliError::ConfigValidation(format!("Failed to render stacker.yml: {}", err))
        })?;
        std::fs::write(&self.file, yaml)?;

        println!(
            "Installed '{}' as project #{}.",
            response.template.slug, response.project.id
        );
        println!("Wrote {}", self.file.display());
        println!(
            "Deploy with: stacker deploy --project {}",
            response.project.name
        );

        Ok(())
    }
}

fn normalize_install_input_key(key: &str) -> String {
    match key.trim() {
        "domain" | "base_domain" => "commonDomain".to_string(),
        other => other.to_string(),
    }
}

fn parse_set_value(entry: &str) -> Result<(String, Value), CliError> {
    let (key, value) = entry.split_once('=').ok_or_else(|| {
        CliError::ConfigValidation(format!(
            "Invalid --set value '{}'. Expected KEY=VALUE.",
            entry
        ))
    })?;
    let key = normalize_install_input_key(key);
    if key.trim().is_empty() {
        return Err(CliError::ConfigValidation(
            "Invalid --set value: key cannot be empty".to_string(),
        ));
    }
    Ok((key, Value::String(value.trim().to_string())))
}

fn resolve_install_inputs(
    mut inputs: Map<String, Value>,
    domain: Option<&str>,
    set_values: &[String],
) -> Result<Map<String, Value>, CliError> {
    if let Some(value) = inputs.remove("base_domain") {
        inputs.entry("commonDomain".to_string()).or_insert(value);
    }
    if let Some(value) = inputs.remove("domain") {
        inputs.entry("commonDomain".to_string()).or_insert(value);
    }
    if let Some(domain) = domain.map(str::trim).filter(|value| !value.is_empty()) {
        inputs.insert(
            "commonDomain".to_string(),
            Value::String(domain.to_ascii_lowercase()),
        );
    }
    for entry in set_values {
        let (key, value) = parse_set_value(entry)?;
        inputs.insert(key, value);
    }
    Ok(inputs)
}

fn apply_install_inputs_to_deploy_form(form: &mut Value, inputs: &Map<String, Value>) {
    if inputs.is_empty() {
        return;
    }
    if let Some(common_domain) = inputs.get("commonDomain").cloned() {
        if let Some(obj) = form.as_object_mut() {
            obj.insert("commonDomain".to_string(), common_domain);
        }
    }
    let Some(stack) = form
        .get_mut("stack")
        .and_then(|value| value.as_object_mut())
    else {
        return;
    };
    let vars = stack
        .entry("vars".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(vars) = vars.as_array_mut() else {
        return;
    };
    for (key, value) in inputs {
        vars.retain(|entry| entry.get("key").and_then(Value::as_str) != Some(key.as_str()));
        vars.push(serde_json::json!({ "key": key, "value": value }));
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace status
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker marketplace status [name] [--json]`
///
/// Check submission status for the user's marketplace templates.
/// If a name is provided, shows detail for that template only.
pub struct MarketplaceStatusCommand {
    name: Option<String>,
    json: bool,
}

impl MarketplaceStatusCommand {
    pub fn new(name: Option<String>, json: bool) -> Self {
        Self { name, json }
    }
}

impl CallableTrait for MarketplaceStatusCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("marketplace status")?;
        let base_url = crate::cli::install_runner::normalize_stacker_server_url(
            crate::cli::stacker_client::DEFAULT_STACKER_URL,
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        let name = self.name.clone();

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let templates = client.marketplace_list_mine().await?;

            if let Some(ref name) = name {
                // Filter to matching template
                let template = templates
                    .iter()
                    .find(|t| t.name == *name || t.slug == *name);
                match template {
                    Some(t) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&t)?);
                        } else {
                            println!(
                                "Stack:      {} v{}",
                                t.name,
                                t.version.as_deref().unwrap_or("?")
                            );
                            println!("Status:     {}", t.status);
                            println!(
                                "Submitted:  {}",
                                t.created_at.as_deref().unwrap_or("\u{2014}")
                            );
                            if let Some(ref reason) = t.review_reason {
                                println!("Reason:     {}", reason);
                            }
                        }
                    }
                    None => {
                        eprintln!("No submission found for '{}'", name);
                        std::process::exit(1);
                    }
                }
            } else {
                if json {
                    println!("{}", serde_json::to_string_pretty(&templates)?);
                } else {
                    if templates.is_empty() {
                        println!("No marketplace submissions found.");
                        println!("Submit your first stack with: stacker submit");
                        return Ok(());
                    }
                    println!(
                        "{:<25} {:<10} {:<15} {:<20}",
                        "STACK", "VERSION", "STATUS", "SUBMITTED"
                    );
                    println!("{}", "\u{2500}".repeat(72));
                    for t in &templates {
                        println!(
                            "{:<25} {:<10} {:<15} {:<20}",
                            truncate(&t.name, 23),
                            t.version.as_deref().unwrap_or("\u{2014}"),
                            t.status,
                            t.created_at.as_deref().unwrap_or("\u{2014}"),
                        );
                    }
                    eprintln!("\n{} submission(s) total.", templates.len());
                }
            }
            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// marketplace logs
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker marketplace logs <name> [--json]`
///
/// Show review comments and history for a marketplace submission.
pub struct MarketplaceLogsCommand {
    name: String,
    json: bool,
}

impl MarketplaceLogsCommand {
    pub fn new(name: String, json: bool) -> Self {
        Self { name, json }
    }
}

impl CallableTrait for MarketplaceLogsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("marketplace logs")?;
        let base_url = crate::cli::install_runner::normalize_stacker_server_url(
            crate::cli::stacker_client::DEFAULT_STACKER_URL,
        );

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        let name = self.name.clone();

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);

            // First, find the template by name to get its ID
            let templates = client.marketplace_list_mine().await?;
            let template = templates.iter().find(|t| t.name == name || t.slug == name);

            let template = match template {
                Some(t) => t,
                None => {
                    eprintln!("No submission found for '{}'", name);
                    std::process::exit(1);
                }
            };

            let reviews = client.marketplace_reviews(&template.id).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&reviews)?);
            } else {
                println!(
                    "Review history for: {} v{}",
                    template.name,
                    template.version.as_deref().unwrap_or("?")
                );
                println!("Current status: {}", template.status);
                println!();

                if reviews.is_empty() {
                    println!("No reviews yet.");
                    return Ok(());
                }

                println!(
                    "{:<12} {:<20} {:<20} {}",
                    "DECISION", "SUBMITTED", "REVIEWED", "REASON"
                );
                println!("{}", "\u{2500}".repeat(80));
                for r in &reviews {
                    println!(
                        "{:<12} {:<20} {:<20} {}",
                        r.decision,
                        r.submitted_at.as_deref().unwrap_or("\u{2014}"),
                        r.reviewed_at.as_deref().unwrap_or("\u{2014}"),
                        r.review_reason.as_deref().unwrap_or(""),
                    );
                }
                eprintln!("\n{} review(s) total.", reviews.len());
            }
            Ok(())
        })
    }
}

// ── helpers ──────────────────────────────────────────

/// Truncate a string to `max_len` characters, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}\u{2026}", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_install_inputs_to_deploy_form, resolve_install_inputs};
    use serde_json::{json, Map, Value};

    #[test]
    fn install_domain_populates_common_domain_and_stack_vars() {
        let inputs = resolve_install_inputs(Map::new(), Some("DIFY.COM"), &[])
            .expect("inputs should resolve");
        assert_eq!(inputs.get("commonDomain"), Some(&json!("dify.com")));

        let mut form = json!({
            "stack": {
                "stack_code": "dify",
                "vars": []
            }
        });
        apply_install_inputs_to_deploy_form(&mut form, &inputs);

        assert_eq!(form["commonDomain"], json!("dify.com"));
        assert_eq!(
            form["stack"]["vars"],
            json!([{ "key": "commonDomain", "value": "dify.com" }])
        );
    }

    #[test]
    fn install_inputs_normalize_base_domain_and_cli_overrides_win() {
        let mut config_inputs = Map::new();
        config_inputs.insert("base_domain".to_string(), json!("example.com"));
        config_inputs.insert("admin_email".to_string(), json!("admin@example.com"));

        let inputs = resolve_install_inputs(
            config_inputs,
            Some("dify.com"),
            &[
                "admin_email=owner@dify.com".to_string(),
                "public_domain=app.dify.com".to_string(),
            ],
        )
        .expect("inputs should resolve");

        assert_eq!(inputs.get("commonDomain"), Some(&json!("dify.com")));
        assert_eq!(inputs.get("admin_email"), Some(&json!("owner@dify.com")));
        assert_eq!(inputs.get("public_domain"), Some(&json!("app.dify.com")));
        assert!(!inputs.contains_key("base_domain"));
    }

    #[test]
    fn apply_install_inputs_replaces_existing_vars() {
        let mut inputs = Map::new();
        inputs.insert("commonDomain".to_string(), json!("new.example.com"));
        inputs.insert("admin_email".to_string(), json!("admin@new.example.com"));
        let mut form = json!({
            "stack": {
                "vars": [
                    { "key": "commonDomain", "value": "old.example.com" },
                    { "key": "keep", "value": "yes" }
                ]
            }
        });

        apply_install_inputs_to_deploy_form(&mut form, &inputs);

        let vars = form["stack"]["vars"]
            .as_array()
            .expect("vars should be array");
        assert_eq!(
            vars.iter()
                .filter(|entry| entry.get("key") == Some(&Value::String("commonDomain".to_string())))
                .count(),
            1
        );
        assert!(vars.contains(&json!({ "key": "keep", "value": "yes" })));
        assert!(vars.contains(&json!({ "key": "commonDomain", "value": "new.example.com" })));
        assert!(vars.contains(&json!({ "key": "admin_email", "value": "admin@new.example.com" })));
    }
}
