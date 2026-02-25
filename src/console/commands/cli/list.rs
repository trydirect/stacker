use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::stacker_client::{self, StackerClient};
use crate::console::commands::CallableTrait;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// list projects
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker list projects [--json]`
///
/// Lists all projects on the Stacker server for the authenticated user.
pub struct ListProjectsCommand {
    pub json: bool,
}

impl ListProjectsCommand {
    pub fn new(json: bool) -> Self {
        Self { json }
    }
}

impl CallableTrait for ListProjectsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("list projects")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let projects = client.list_projects().await?;

            if projects.is_empty() {
                eprintln!("No projects found.");
                return Ok(());
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&projects)?);
            } else {
                // Table header
                println!(
                    "{:<6} {:<30} {:<26} {:<26}",
                    "ID", "NAME", "CREATED", "UPDATED"
                );
                println!("{}", "─".repeat(90));

                for p in &projects {
                    println!(
                        "{:<6} {:<30} {:<26} {:<26}",
                        p.id,
                        truncate(&p.name, 28),
                        &p.created_at,
                        &p.updated_at,
                    );
                }

                eprintln!("\n{} project(s) total.", projects.len());
            }

            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// list servers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker list servers [--json]`
///
/// Lists all servers on the Stacker server for the authenticated user.
pub struct ListServersCommand {
    pub json: bool,
}

impl ListServersCommand {
    pub fn new(json: bool) -> Self {
        Self { json }
    }
}

impl CallableTrait for ListServersCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("list servers")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let servers = client.list_servers().await?;

            if servers.is_empty() {
                eprintln!("No servers found.");
                return Ok(());
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&servers)?);
            } else {
                println!(
                    "{:<6} {:<20} {:<16} {:<10} {:<10} {:<10} {:<12} {:<10}",
                    "ID", "NAME", "IP", "CLOUD", "REGION", "SIZE", "KEY STATUS", "MODE"
                );
                println!("{}", "─".repeat(100));

                for s in &servers {
                    println!(
                        "{:<6} {:<20} {:<16} {:<10} {:<10} {:<10} {:<12} {:<10}",
                        s.id,
                        truncate(&s.name.clone().unwrap_or_else(|| "-".to_string()), 18),
                        s.srv_ip.clone().unwrap_or_else(|| "-".to_string()),
                        s.cloud.clone().unwrap_or_else(|| "-".to_string()),
                        truncate(&s.region.clone().unwrap_or_else(|| "-".to_string()), 8),
                        truncate(&s.server.clone().unwrap_or_else(|| "-".to_string()), 8),
                        &s.key_status,
                        &s.connection_mode,
                    );
                }

                eprintln!("\n{} server(s) total.", servers.len());
            }

            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// list ssh-keys
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker list ssh-keys [--json]`
///
/// Lists all servers and their SSH key status. SSH keys are managed
/// per-server, so this command shows each server's key state.
pub struct ListSshKeysCommand {
    pub json: bool,
}

impl ListSshKeysCommand {
    pub fn new(json: bool) -> Self {
        Self { json }
    }
}

impl CallableTrait for ListSshKeysCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("list ssh-keys")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let servers = client.list_servers().await?;

            if servers.is_empty() {
                eprintln!("No servers found (SSH keys are managed per-server).");
                return Ok(());
            }

            if json {
                // Output a focused JSON view with just SSH key info
                let ssh_info: Vec<serde_json::Value> = servers
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "server_id": s.id,
                            "server_name": s.name,
                            "srv_ip": s.srv_ip,
                            "ssh_port": s.ssh_port,
                            "ssh_user": s.ssh_user,
                            "key_status": s.key_status,
                            "connection_mode": s.connection_mode,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&ssh_info)?);
            } else {
                println!(
                    "{:<6} {:<20} {:<16} {:<8} {:<10} {:<12} {:<10}",
                    "ID", "SERVER", "IP", "PORT", "USER", "KEY STATUS", "MODE"
                );
                println!("{}", "─".repeat(84));

                let mut active_count = 0;
                for s in &servers {
                    let status_icon = match s.key_status.as_str() {
                        "active" => { active_count += 1; "✓ active" },
                        "pending" => "◷ pending",
                        "failed" => "✗ failed",
                        _ => "  none",
                    };
                    println!(
                        "{:<6} {:<20} {:<16} {:<8} {:<10} {:<12} {:<10}",
                        s.id,
                        truncate(&s.name.clone().unwrap_or_else(|| "-".to_string()), 18),
                        s.srv_ip.clone().unwrap_or_else(|| "-".to_string()),
                        s.ssh_port.map(|p| p.to_string()).unwrap_or_else(|| "22".to_string()),
                        s.ssh_user.clone().unwrap_or_else(|| "root".to_string()),
                        status_icon,
                        &s.connection_mode,
                    );
                }

                eprintln!(
                    "\n{} server(s), {} with active SSH keys.",
                    servers.len(),
                    active_count
                );
            }

            Ok(())
        })
    }
}

// ── helpers ──────────────────────────────────────────

/// Truncate a string to `max_len` characters, adding "…" if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}
