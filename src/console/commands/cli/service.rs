//! Service management commands — add services from templates to stacker.yml.
//!
//! `stacker service add <name>` resolves a service template from the catalog
//! (hardcoded or marketplace API) and appends it to the `services` section of
//! `stacker.yml`.
//!
//! `stacker service list [--online]` shows available service templates.

use std::path::{Path, PathBuf};

use crate::cli::compose_service_sync::{
    sync_configured_compose_services, ComposeServiceSyncResult,
};
use crate::cli::config_parser::{ServiceDefinition, StackerConfig};
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::service_catalog::{catalog_inputs, CatalogInput, InputDefault, ServiceCatalog};
use crate::cli::service_import::{
    import_plan_from_compose_file, parse_renames, ComposeImportRequest, ServiceImportPlan,
    ServiceImportReview,
};
use crate::cli::stacker_client::{self, StackerClient};
use crate::console::commands::CallableTrait;
use dialoguer::{Confirm, FuzzySelect, Input, Password};
use serde::Serialize;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service add
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service add <name> [--file <stacker.yml>]`
///
/// Resolves a service template (e.g. "postgres", "redis", "wordpress") and
/// appends it to the `services` array in stacker.yml.
pub struct ServiceAddCommand {
    pub name: Option<String>,
    pub file: Option<String>,
}

impl ServiceAddCommand {
    pub fn new(name: Option<String>, file: Option<String>) -> Self {
        Self { name, file }
    }
}

impl CallableTrait for ServiceAddCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.file.as_deref().unwrap_or(DEFAULT_CONFIG_FILE);
        let path = Path::new(config_path);

        if !path.exists() {
            return Err(Box::new(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            }));
        }

        // Load existing config without resolving ${VAR} placeholders so
        // that sensitive values from .env are not written back to the file.
        let mut config = StackerConfig::from_file_raw(path)?;

        // Resolve name — either from arg or interactive fuzzy picker
        let chosen_name = match &self.name {
            Some(n) => n.clone(),
            None => {
                let catalog = ServiceCatalog::offline();
                let entries = catalog.list_available();
                let display: Vec<String> = entries
                    .iter()
                    .map(|e| format!("{:<22} [{:<10}] {}", e.code, e.category, e.description))
                    .collect();
                let idx = FuzzySelect::new()
                    .with_prompt("Select a service to add")
                    .items(&display)
                    .default(0)
                    .interact()
                    .map_err(|e| CliError::ConfigValidation(format!("Picker error: {}", e)))?;
                entries[idx].code.clone()
            }
        };

        // Resolve canonical name
        let canonical = ServiceCatalog::resolve_alias(&chosen_name);

        // Check for duplicates
        if config.services.iter().any(|s| s.name == canonical) {
            eprintln!(
                "⚠ Service '{}' already exists in {}. Skipping.",
                canonical, config_path
            );
            return Ok(());
        }

        // Try to create a catalog with online access, fall back to offline
        let catalog = match try_build_online_catalog() {
            Some(client) => ServiceCatalog::new(Some(client)),
            None => ServiceCatalog::offline(),
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                CliError::ConfigValidation(format!("Failed to create async runtime: {}", e))
            })?;

        let entry = rt.block_on(catalog.resolve(&canonical))?;

        // Add missing dependencies first, then the requested service, tracking
        // the canonical codes so we can resolve their configurable inputs.
        let mut added_codes: Vec<String> = Vec::new();
        for dep in &entry.service.depends_on {
            if !config.services.iter().any(|s| &s.name == dep) {
                // Try to resolve the dependency too
                if let Ok(dep_entry) = rt.block_on(catalog.resolve(dep)) {
                    eprintln!(
                        "  + Adding dependency: {} ({})",
                        dep_entry.name, dep_entry.service.image
                    );
                    config.services.push(dep_entry.service);
                    added_codes.push(dep_entry.code);
                }
            }
        }
        config.services.push(entry.service.clone());
        added_codes.push(entry.code.clone());

        // Resolve configurable inputs (passwords, etc.) for the added services
        // and persist them to `.env`. The service definitions reference `${KEY}`,
        // so secrets never land in stacker.yml. Existing `.env` keys are only
        // overwritten with the user's permission.
        let inputs: Vec<CatalogInput> = added_codes
            .iter()
            .flat_map(|code| catalog_inputs(code))
            .collect();
        if !inputs.is_empty() {
            let env_path = project_dir_for_config(path).join(".env");
            apply_service_inputs(&env_path, &inputs)?;
        }

        // Serialize back to YAML
        let yaml = serde_yaml::to_string(&config).map_err(|e| {
            CliError::ConfigValidation(format!("Failed to serialize config: {}", e))
        })?;

        // Backup and write
        let backup_path = format!("{}.bak", config_path);
        std::fs::copy(config_path, &backup_path)?;
        std::fs::write(config_path, &yaml)?;
        let compose_sync = sync_configured_compose_services(
            &project_dir_for_config(path),
            &config,
            std::slice::from_ref(&entry.service.name),
        )?;

        println!("✓ Added '{}' to {}", entry.name, config_path);
        println!("  Image:  {}", entry.service.image);
        if !entry.service.ports.is_empty() {
            println!("  Ports:  {}", entry.service.ports.join(", "));
        }
        if !entry.service.volumes.is_empty() {
            println!("  Volumes: {}", entry.service.volumes.join(", "));
        }
        if !entry.service.environment.is_empty() {
            println!(
                "  Env vars: {}",
                entry
                    .service
                    .environment
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !entry.related.is_empty() {
            let missing_related: Vec<&str> = entry
                .related
                .iter()
                .filter(|r| !config.services.iter().any(|s| &s.name == *r))
                .map(|r| r.as_str())
                .collect();
            if !missing_related.is_empty() {
                eprintln!();
                eprintln!(
                    "  💡 Related services you might also want: {}",
                    missing_related.join(", ")
                );
            }
        }

        eprintln!("  Backup saved to {}", backup_path);
        print_compose_sync_result(&compose_sync);

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service deploy
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service deploy <name> [--deployment <hash>]`
///
/// Validates that the named service exists in `stacker.yml`, then delegates to
/// the lower-level agent app deployment command using the service name as the
/// remote app code.
pub struct ServiceDeployCommand {
    pub name: String,
    pub force: bool,
    pub runtime: String,
    pub json: bool,
    pub deployment: Option<String>,
    pub environment: Option<String>,
    pub plan: bool,
    pub apply_plan: Option<String>,
}

impl ServiceDeployCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        force: bool,
        runtime: String,
        json: bool,
        deployment: Option<String>,
        environment: Option<String>,
        plan: bool,
        apply_plan: Option<String>,
    ) -> Self {
        Self {
            name,
            force,
            runtime,
            json,
            deployment,
            environment,
            plan,
            apply_plan,
        }
    }
}

impl CallableTrait for ServiceDeployCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = DEFAULT_CONFIG_FILE;
        let path = Path::new(config_path);

        if !path.exists() {
            return Err(Box::new(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            }));
        }

        let config = StackerConfig::from_file_raw(path)?;
        if !config
            .services
            .iter()
            .any(|service| service.name == self.name)
        {
            return Err(Box::new(CliError::ConfigValidation(format!(
                "Service '{}' was not found in {}. Add or import it first, then run `stacker service deploy {}`.",
                self.name, config_path, self.name
            ))));
        }

        let compose_sync = sync_configured_compose_services(
            &project_dir_for_config(path),
            &config,
            std::slice::from_ref(&self.name),
        )?;
        print_compose_sync_result(&compose_sync);

        let environment = self.environment.clone().or_else(|| {
            if config.selected_environment(None).is_none() && config.deploy.compose_file.is_some() {
                eprintln!(
                    "  No deploy environment configured; using 'production' to build the service compose payload."
                );
                Some("production".to_string())
            } else {
                None
            }
        });

        let command = crate::console::commands::cli::agent::AgentDeployAppCommand::new(
            self.name.clone(),
            None,
            self.force,
            self.runtime.clone(),
            self.json,
            self.deployment.clone(),
            environment,
        )
        .with_plan(self.plan)
        .with_apply_plan(self.apply_plan.clone());

        command.call()
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service import
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service import <name> --from-compose <path> [--service <compose-service>]`
///
/// Parses a local Docker Compose file, prints a safety review, and appends
/// selected image-backed services to `stacker.yml` only after confirmation.
pub struct ServiceImportCommand {
    pub name: String,
    pub from_compose: Option<PathBuf>,
    pub from_github: Option<String>,
    pub from_url: Option<String>,
    pub service: Option<String>,
    pub renames: Vec<String>,
    pub file: Option<String>,
    pub review: bool,
    pub yes: bool,
    pub json: bool,
}

impl ServiceImportCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        from_compose: Option<PathBuf>,
        from_github: Option<String>,
        from_url: Option<String>,
        service: Option<String>,
        renames: Vec<String>,
        file: Option<String>,
        review: bool,
        yes: bool,
        json: bool,
    ) -> Self {
        Self {
            name,
            from_compose,
            from_github,
            from_url,
            service,
            renames,
            file,
            review,
            yes,
            json,
        }
    }
}

impl CallableTrait for ServiceImportCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.file.as_deref().unwrap_or(DEFAULT_CONFIG_FILE);
        let path = Path::new(config_path);

        if self.from_github.is_some() || self.from_url.is_some() {
            return Err(Box::new(CliError::ConfigValidation(
                "Remote custom service import is planned but not implemented yet. Download or inspect the Compose file yourself, then run `stacker service import <name> --from-compose <path> --review`."
                    .to_string(),
            )));
        }

        let compose_path = self.from_compose.as_ref().ok_or_else(|| {
            CliError::ConfigValidation(
                "Specify a local Compose file with --from-compose <path>. Remote GitHub/URL import is not fetched by default."
                    .to_string(),
            )
        })?;

        if !path.exists() {
            return Err(Box::new(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            }));
        }

        let renames = parse_renames(&self.renames)?;
        let request = ComposeImportRequest {
            import_name: self.name.clone(),
            selected_service: self.service.clone(),
            renames,
        };
        let plan = import_plan_from_compose_file(compose_path, &request)?;
        let config = StackerConfig::from_file_raw(path)?;
        validate_no_duplicate_services(&config, &plan)?;

        if self.json && self.review {
            let output = ServiceImportCommandOutput {
                status: "review",
                config_file: config_path.to_string(),
                backup_file: None,
                review: &plan.review,
                imported_services: plan
                    .services
                    .iter()
                    .map(|service| service.name.clone())
                    .collect(),
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else if !self.json {
            print_import_plan(&plan);
        }

        if self.review {
            return Ok(());
        }

        if !self.yes {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "Import {} service(s) into {}?",
                    plan.services.len(),
                    config_path
                ))
                .default(false)
                .interact()
                .map_err(|e| {
                    CliError::ConfigValidation(format!(
                        "Prompt failed: {e}. Re-run with --review to inspect only, or --yes to import non-interactively."
                    ))
                })?;

            if !confirmed {
                println!("Aborted.");
                return Ok(());
            }
        }

        let backup_path = import_services_into_config(path, config, &plan)?;
        let updated_config = StackerConfig::from_file_raw(path)?;
        let imported_service_names: Vec<String> = plan
            .services
            .iter()
            .map(|service| service.name.clone())
            .collect();
        let compose_sync = sync_configured_compose_services(
            &project_dir_for_config(path),
            &updated_config,
            &imported_service_names,
        )?;

        if self.json {
            let output = ServiceImportCommandOutput {
                status: "imported",
                config_file: config_path.to_string(),
                backup_file: Some(backup_path.clone()),
                review: &plan.review,
                imported_services: updated_config
                    .services
                    .iter()
                    .filter(|service| {
                        plan.services
                            .iter()
                            .any(|imported| imported.name == service.name)
                    })
                    .map(|service| service.name.clone())
                    .collect(),
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!(
                "✓ Imported {} service(s) into {}",
                plan.services.len(),
                config_path
            );
            eprintln!("  Backup saved to {}", backup_path);
            print_compose_sync_result(&compose_sync);
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service list
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service list [--online]`
///
/// Lists all available service templates from the hardcoded catalog.
/// With `--online`, also queries the marketplace API.
pub struct ServiceListCommand {
    pub online: bool,
}

impl ServiceListCommand {
    pub fn new(online: bool) -> Self {
        Self { online }
    }
}

impl CallableTrait for ServiceListCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let catalog = ServiceCatalog::offline();
        let entries = catalog.list_available();

        // Group by category
        let mut by_category: std::collections::BTreeMap<String, Vec<_>> =
            std::collections::BTreeMap::new();
        for entry in &entries {
            by_category
                .entry(entry.category.clone())
                .or_default()
                .push(entry);
        }

        println!("Available service templates:");
        println!();

        for (category, services) in &by_category {
            println!("  {} {}:", category_icon(category), capitalize(category));
            for svc in services {
                println!(
                    "    {:<22} {:<30} {}",
                    svc.code, svc.service.image, svc.description
                );
            }
            println!();
        }

        println!("Usage: stacker service add <name>");
        println!("Aliases: wp, pg, my, mongo, es, mq, pma, smtp, mail, mh");

        if self.online {
            eprintln!();
            eprintln!("Marketplace templates:");
            match try_build_online_catalog() {
                Some(client) => {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| {
                            CliError::ConfigValidation(format!(
                                "Failed to create async runtime: {}",
                                e
                            ))
                        })?;

                    match rt.block_on(client.list_marketplace_templates(None, None)) {
                        Ok(templates) if templates.is_empty() => {
                            eprintln!("  (no marketplace templates available)");
                        }
                        Ok(templates) => {
                            for t in &templates {
                                eprintln!(
                                    "  {:<22} {}",
                                    t.slug,
                                    t.description.as_deref().unwrap_or(""),
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("  (failed to fetch: {})", e);
                        }
                    }
                }
                None => {
                    eprintln!("  (requires login: stacker login)");
                }
            }
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service remove
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service remove <name> [--file <stacker.yml>]`
///
/// Removes a service entry from the `services` array in stacker.yml after
/// confirming with the user.
pub struct ServiceRemoveCommand {
    pub name: String,
    pub file: Option<String>,
}

impl ServiceRemoveCommand {
    pub fn new(name: String, file: Option<String>) -> Self {
        Self { name, file }
    }
}

impl CallableTrait for ServiceRemoveCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.file.as_deref().unwrap_or(DEFAULT_CONFIG_FILE);
        let path = Path::new(config_path);

        if !path.exists() {
            return Err(Box::new(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            }));
        }

        let mut config = StackerConfig::from_file_raw(path)?;
        let canonical = ServiceCatalog::resolve_alias(&self.name);

        if !config.services.iter().any(|s| s.name == canonical) {
            eprintln!("⚠ Service '{}' not found in {}.", canonical, config_path);
            return Ok(());
        }

        let confirmed = Confirm::new()
            .with_prompt(format!("Remove '{}' from {}?", canonical, config_path))
            .default(false)
            .interact()
            .map_err(|e| CliError::ConfigValidation(format!("Prompt error: {}", e)))?;

        if !confirmed {
            println!("Aborted.");
            return Ok(());
        }

        config.services.retain(|s| s.name != canonical);

        let yaml = serde_yaml::to_string(&config).map_err(|e| {
            CliError::ConfigValidation(format!("Failed to serialize config: {}", e))
        })?;

        let backup_path = format!("{}.bak", config_path);
        std::fs::copy(config_path, &backup_path)?;
        std::fs::write(config_path, &yaml)?;

        println!("✓ Removed '{}' from {}", canonical, config_path);
        eprintln!("  Backup saved to {}", backup_path);

        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────

#[derive(Serialize)]
struct ServiceImportCommandOutput<'a> {
    status: &'static str,
    config_file: String,
    backup_file: Option<String>,
    review: &'a ServiceImportReview,
    imported_services: Vec<String>,
}

fn validate_no_duplicate_services(
    config: &StackerConfig,
    plan: &ServiceImportPlan,
) -> Result<(), CliError> {
    for imported in &plan.services {
        if config.services.iter().any(|svc| svc.name == imported.name) {
            return Err(CliError::ConfigValidation(format!(
                "Service '{}' already exists in stacker.yml. Use --rename old=new or choose a different import name.",
                imported.name
            )));
        }
    }
    Ok(())
}

fn import_services_into_config(
    path: &Path,
    mut config: StackerConfig,
    plan: &ServiceImportPlan,
) -> Result<String, Box<dyn std::error::Error>> {
    for service in &plan.services {
        config.services.push(service.clone());
    }

    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;

    let config_path = path.to_string_lossy().to_string();
    let backup_path = format!("{}.bak", config_path);
    std::fs::copy(path, &backup_path)?;
    std::fs::write(path, &yaml)?;
    Ok(backup_path)
}

fn print_import_plan(plan: &ServiceImportPlan) {
    let review = &plan.review;
    println!("Custom service import review: {}", review.import_name);
    println!();

    for service in &review.services {
        println!("  Service: {} (from {})", service.name, service.source_name);
        println!("    Image: {}", service.image);
        if !service.ports.is_empty() {
            println!("    Ports: {}", service.ports.join(", "));
        }
        if !service.environment_keys.is_empty() {
            println!("    Env keys: {}", service.environment_keys.join(", "));
        }
        if !service.volumes.is_empty() {
            println!("    Volumes: {}", service.volumes.join(", "));
        }
        if !service.depends_on.is_empty() {
            println!("    Depends on: {}", service.depends_on.join(", "));
        }
        if !service.unsupported_fields.is_empty() {
            println!(
                "    Unsupported Compose fields: {}",
                service.unsupported_fields.join(", ")
            );
        }
    }

    if !review.risks.is_empty() {
        println!();
        println!("  Risks to review:");
        for risk in &review.risks {
            println!("    - [{}] {}: {}", risk.service, risk.kind, risk.detail);
        }
    }

    if !review.guidance.is_empty() {
        println!();
        println!("  Guidance:");
        for item in &review.guidance {
            println!("    - {}", item);
        }
    }

    if let Ok(yaml) = serde_yaml::to_string(&plan.services) {
        println!();
        println!("  stacker.yml services to append:");
        for line in yaml.lines() {
            println!("    {}", line);
        }
    }
}

fn project_dir_for_config(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Read the top-level keys already defined in a `.env` file (ignoring comments
/// and blanks). Returns an empty set if the file doesn't exist.
fn env_file_keys(env_path: &Path) -> std::collections::HashSet<String> {
    std::fs::read_to_string(env_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=').map(|(key, _)| key.trim().to_string())
        })
        .collect()
}

/// Generate a random 32-char alphanumeric secret (no shell-special chars).
fn generate_secret() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Resolve a single catalog input to a value (generate / prompt / default).
fn resolve_input_value(input: &CatalogInput, interactive: bool) -> Result<String, CliError> {
    let prompt_err = |e| CliError::ConfigValidation(format!("Prompt failed: {e}"));
    match &input.default {
        InputDefault::GeneratedSecret => Ok(generate_secret()),
        InputDefault::Literal(default) if interactive => Input::new()
            .with_prompt(&input.prompt)
            .default(default.clone())
            .interact_text()
            .map_err(prompt_err),
        InputDefault::Literal(default) => Ok(default.clone()),
        InputDefault::Required if !interactive => Err(CliError::ConfigValidation(format!(
            "Input '{}' is required; re-run interactively to provide it.",
            input.key
        ))),
        InputDefault::Required if input.secret => Password::new()
            .with_prompt(&input.prompt)
            .interact()
            .map_err(prompt_err),
        InputDefault::Required => Input::new()
            .with_prompt(&input.prompt)
            .interact_text()
            .map_err(prompt_err),
    }
}

/// Resolve catalog inputs and persist them to `.env`, referenced as `${KEY}` in
/// the service definitions. Keys already present in `.env` are only overwritten
/// after the user confirms (never in a non-interactive session).
fn apply_service_inputs(env_path: &Path, inputs: &[CatalogInput]) -> Result<(), CliError> {
    use std::io::IsTerminal;
    let interactive = std::io::stdin().is_terminal();
    let existing_keys = env_file_keys(env_path);

    let conflicting: Vec<&str> = inputs
        .iter()
        .map(|input| input.key.as_str())
        .filter(|key| existing_keys.contains(*key))
        .collect();

    let overwrite = if conflicting.is_empty() {
        true
    } else if !interactive {
        eprintln!(
            "  ℹ {} already set in {} — keeping existing value(s) (re-run interactively to overwrite).",
            conflicting.join(", "),
            env_path.display()
        );
        false
    } else {
        Confirm::new()
            .with_prompt(format!(
                "{} already set in {}. Overwrite?",
                conflicting.join(", "),
                env_path.display()
            ))
            .default(false)
            .interact()
            .map_err(|e| CliError::ConfigValidation(format!("Prompt failed: {e}")))?
    };

    let mut updates: Vec<(String, String)> = Vec::new();
    for input in inputs {
        if existing_keys.contains(&input.key) && !overwrite {
            eprintln!("  = {} (kept existing)", input.key);
            continue;
        }
        updates.push((input.key.clone(), resolve_input_value(input, interactive)?));
    }

    if updates.is_empty() {
        return Ok(());
    }
    write_env_updates(env_path, &updates)?;
    for (key, _) in &updates {
        eprintln!("  ✓ {} → {}", key, env_path.display());
    }
    Ok(())
}

/// Upsert `KEY=value` lines into `.env`, preserving all unrelated lines. Creates
/// the file if missing.
fn write_env_updates(env_path: &Path, updates: &[(String, String)]) -> Result<(), CliError> {
    let existing = std::fs::read_to_string(env_path).unwrap_or_default();
    let update_map: std::collections::HashMap<&str, &str> = updates
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut written: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out_lines: Vec<String> = Vec::new();

    for line in existing.lines() {
        if let Some((key, _)) = line.trim_start().split_once('=') {
            let key = key.trim();
            if let Some(value) = update_map.get(key) {
                out_lines.push(format!("{}={}", key, value));
                written.insert(key);
                continue;
            }
        }
        out_lines.push(line.to_string());
    }
    for (key, value) in updates {
        if !written.contains(key.as_str()) {
            out_lines.push(format!("{}={}", key, value));
        }
    }

    let mut content = out_lines.join("\n");
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    std::fs::write(env_path, content)?;
    Ok(())
}

fn print_compose_sync_result(result: &ComposeServiceSyncResult) {
    if result.updated_services.is_empty() {
        return;
    }
    if let Some(path) = result.compose_path.as_ref() {
        eprintln!(
            "  Updated compose file {} with service(s): {}",
            path.display(),
            result.updated_services.join(", ")
        );
    }
    if let Some(path) = result.backup_path.as_ref() {
        eprintln!("  Compose backup saved to {}", path.display());
    }
}

/// Try to build a `StackerClient` from stored credentials (best-effort).
fn try_build_online_catalog() -> Option<StackerClient> {
    let cred_manager = CredentialsManager::with_default_store();
    let creds = cred_manager.require_valid_token("service catalog").ok()?;
    Some(StackerClient::new(
        stacker_client::DEFAULT_STACKER_URL,
        &creds.access_token,
    ))
}

fn category_icon(category: &str) -> &str {
    match category {
        "database" => "🗄",
        "cache" => "⚡",
        "queue" => "📨",
        "proxy" => "🔀",
        "web" => "🌐",
        "search" => "🔍",
        "monitoring" => "📊",
        "devtool" => "🛠",
        "storage" => "💾",
        "mail" => "✉",
        _ => "📦",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_config(dir: &TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("stacker.yml");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn generate_secret_is_long_and_alphanumeric() {
        let s = generate_secret();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(s, generate_secret(), "secrets should differ");
    }

    #[test]
    fn env_file_keys_ignores_comments_and_blanks() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "# comment\n\nFOO=1\nBAR = two\n").unwrap();
        let keys = env_file_keys(&path);
        assert!(keys.contains("FOO"));
        assert!(keys.contains("BAR"));
        assert_eq!(keys.len(), 2);
        // Missing file → empty set.
        assert!(env_file_keys(&dir.path().join("nope.env")).is_empty());
    }

    #[test]
    fn write_env_updates_upserts_and_preserves_other_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "# header\nKEEP=me\nPOSTGRES_PASSWORD=old\n").unwrap();

        write_env_updates(
            &path,
            &[
                ("POSTGRES_PASSWORD".to_string(), "new".to_string()),
                ("REDIS_PASSWORD".to_string(), "fresh".to_string()),
            ],
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# header"), "comment preserved");
        assert!(content.contains("KEEP=me"), "unrelated key preserved");
        assert!(
            content.contains("POSTGRES_PASSWORD=new"),
            "existing key replaced"
        );
        assert!(!content.contains("POSTGRES_PASSWORD=old"), "old value gone");
        assert!(content.contains("REDIS_PASSWORD=fresh"), "new key appended");
    }

    #[test]
    fn write_env_updates_creates_file_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        write_env_updates(&path, &[("K".to_string(), "v".to_string())]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "K=v\n");
    }

    fn write_compose(dir: &TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("compose.yml");
        std::fs::write(&path, body).unwrap();
        path
    }

    fn import_command(
        config_path: &Path,
        compose_path: &Path,
        review: bool,
        yes: bool,
    ) -> ServiceImportCommand {
        ServiceImportCommand::new(
            "smtp".to_string(),
            Some(compose_path.to_path_buf()),
            None,
            None,
            Some("mailserver".to_string()),
            Vec::new(),
            Some(config_path.to_string_lossy().to_string()),
            review,
            yes,
            false,
        )
    }

    #[test]
    fn service_import_review_only_does_not_write_config_or_backup() {
        let dir = TempDir::new().unwrap();
        let config_path = write_config(
            &dir,
            r#"
name: test-app
app:
  type: static
services: []
"#,
        );
        let compose_path = write_compose(
            &dir,
            r#"
services:
  mailserver:
    image: docker.io/mailserver/docker-mailserver:latest
    environment:
      ACCOUNT_PASSWORD: literal-secret
"#,
        );
        let original = std::fs::read_to_string(&config_path).unwrap();

        import_command(&config_path, &compose_path, true, false)
            .call()
            .unwrap();

        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), original);
        assert!(!Path::new(&format!("{}.bak", config_path.to_string_lossy())).exists());
    }

    #[test]
    fn service_import_prevents_duplicate_service_names() {
        let dir = TempDir::new().unwrap();
        let config_path = write_config(
            &dir,
            r#"
name: test-app
app:
  type: static
services:
  - name: smtp
    image: trydirect/smtp
"#,
        );
        let compose_path = write_compose(
            &dir,
            r#"
services:
  mailserver:
    image: docker.io/mailserver/docker-mailserver:latest
"#,
        );

        let err = import_command(&config_path, &compose_path, false, true)
            .call()
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn service_import_writes_backup_and_preserves_secret_placeholders() {
        let dir = TempDir::new().unwrap();
        let config_path = write_config(
            &dir,
            r#"
name: test-app
app:
  type: static
services: []
"#,
        );
        let compose_path = write_compose(
            &dir,
            r#"
services:
  mailserver:
    image: docker.io/mailserver/docker-mailserver:latest
    ports:
      - "25:25"
    environment:
      ACCOUNT_PASSWORD: literal-secret
      POSTMASTER_ADDRESS: postmaster@example.com
    volumes:
      - maildata:/var/mail
"#,
        );

        import_command(&config_path, &compose_path, false, true)
            .call()
            .unwrap();

        let backup_path = format!("{}.bak", config_path.to_string_lossy());
        assert!(Path::new(&backup_path).exists());
        let config = StackerConfig::from_file_raw(&config_path).unwrap();
        let service = config
            .services
            .iter()
            .find(|service| service.name == "smtp")
            .unwrap();
        assert_eq!(service.ports, vec!["25:25"]);
        assert_eq!(
            service.environment.get("ACCOUNT_PASSWORD").unwrap(),
            "${ACCOUNT_PASSWORD}"
        );
        assert_eq!(
            service.environment.get("POSTMASTER_ADDRESS").unwrap(),
            "postmaster@example.com"
        );
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
