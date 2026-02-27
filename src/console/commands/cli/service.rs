//! Service management commands — add services from templates to stacker.yml.
//!
//! `stacker service add <name>` resolves a service template from the catalog
//! (hardcoded or marketplace API) and appends it to the `services` section of
//! `stacker.yml`.
//!
//! `stacker service list [--online]` shows available service templates.

use std::path::Path;

use crate::cli::config_parser::{StackerConfig, ServiceDefinition};
use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::service_catalog::ServiceCatalog;
use crate::cli::stacker_client::{self, StackerClient};
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// service add
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker service add <name> [--file <stacker.yml>]`
///
/// Resolves a service template (e.g. "postgres", "redis", "wordpress") and
/// appends it to the `services` array in stacker.yml.
pub struct ServiceAddCommand {
    pub name: String,
    pub file: Option<String>,
}

impl ServiceAddCommand {
    pub fn new(name: String, file: Option<String>) -> Self {
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

        // Load existing config
        let mut config = StackerConfig::from_file(path)?;

        // Resolve canonical name
        let canonical = ServiceCatalog::resolve_alias(&self.name);

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
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        let entry = rt.block_on(catalog.resolve(&canonical))?;

        // Check if the service has dependencies that are missing
        let mut services_to_add: Vec<ServiceDefinition> = Vec::new();
        for dep in &entry.service.depends_on {
            if !config.services.iter().any(|s| &s.name == dep) {
                // Try to resolve the dependency too
                if let Ok(dep_entry) = rt.block_on(catalog.resolve(dep)) {
                    eprintln!(
                        "  + Adding dependency: {} ({})",
                        dep_entry.name, dep_entry.service.image
                    );
                    services_to_add.push(dep_entry.service);
                }
            }
        }

        // Add dependencies first, then the requested service
        for dep_svc in services_to_add {
            config.services.push(dep_svc);
        }
        config.services.push(entry.service.clone());

        // Serialize back to YAML
        let yaml = serde_yaml::to_string(&config)
            .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;

        // Backup and write
        let backup_path = format!("{}.bak", config_path);
        std::fs::copy(config_path, &backup_path)?;
        std::fs::write(config_path, &yaml)?;

        println!("✓ Added '{}' to {}", entry.name, config_path);
        println!("  Image:  {}", entry.service.image);
        if !entry.service.ports.is_empty() {
            println!("  Ports:  {}", entry.service.ports.join(", "));
        }
        if !entry.service.volumes.is_empty() {
            println!("  Volumes: {}", entry.service.volumes.join(", "));
        }
        if !entry.service.environment.is_empty() {
            println!("  Env vars: {}", entry.service.environment.keys()
                .cloned().collect::<Vec<_>>().join(", "));
        }
        if !entry.related.is_empty() {
            let missing_related: Vec<&str> = entry.related.iter()
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
        let mut by_category: std::collections::BTreeMap<String, Vec<_>> = std::collections::BTreeMap::new();
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
        println!("Aliases: wp, pg, my, mongo, es, mq, pma, mh");

        if self.online {
            eprintln!();
            eprintln!("Marketplace templates:");
            match try_build_online_catalog() {
                Some(client) => {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

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

// ── Helpers ──────────────────────────────────────────

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

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
