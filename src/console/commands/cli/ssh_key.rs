//! SSH key management commands — generate, show (read), and upload (update) keys.
//!
//! All operations call the Stacker server REST API (`/server/{id}/ssh-key/*`)
//! which stores keys in HashiCorp Vault. Requires `stacker login` first.

use std::path::PathBuf;

use crate::cli::credentials::CredentialsManager;
use crate::cli::error::CliError;
use crate::cli::stacker_client::{self, StackerClient};
use crate::console::commands::CallableTrait;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ssh-key generate
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker ssh-key generate --server-id <ID> [--save-to <PATH>]`
///
/// Generates a new Ed25519 SSH key pair on the server and stores it in Vault.
/// Prints the public key and fingerprint. If Vault storage fails, the server
/// returns the private key inline — use `--save-to` to save it to a local file.
pub struct SshKeyGenerateCommand {
    pub server_id: i32,
    pub save_to: Option<PathBuf>,
}

impl SshKeyGenerateCommand {
    pub fn new(server_id: i32, save_to: Option<PathBuf>) -> Self {
        Self { server_id, save_to }
    }
}

impl CallableTrait for SshKeyGenerateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let server_id = self.server_id;
        let save_to = self.save_to.clone();

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("ssh-key generate")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let result = client.generate_ssh_key(server_id).await?;

            println!("✓ SSH key generated for server {}", server_id);
            println!();
            println!("  Public key:");
            println!("    {}", result.public_key);
            if let Some(fp) = &result.fingerprint {
                println!("  Fingerprint: {}", fp);
            }
            println!("  Message: {}", result.message);

            // If the private key was returned (Vault storage failed), offer to save it
            if let Some(private_key) = &result.private_key {
                eprintln!();
                eprintln!("  ⚠ Vault storage failed — private key returned inline.");
                if let Some(path) = save_to {
                    std::fs::write(&path, private_key)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
                    }
                    eprintln!("  ✓ Private key saved to {} (mode 600)", path.display());
                } else {
                    eprintln!("  Use --save-to <path> to save the private key to a file.");
                }
            }

            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ssh-key show
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker ssh-key show --server-id <ID> [--json]`
///
/// Retrieves the public SSH key for a server from Vault.
pub struct SshKeyShowCommand {
    pub server_id: i32,
    pub json: bool,
}

impl SshKeyShowCommand {
    pub fn new(server_id: i32, json: bool) -> Self {
        Self { server_id, json }
    }
}

impl CallableTrait for SshKeyShowCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let server_id = self.server_id;
        let json = self.json;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("ssh-key show")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let result = client.get_ssh_public_key(server_id).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("SSH public key for server {}:", server_id);
                println!();
                println!("{}", result.public_key);
                if let Some(fp) = &result.fingerprint {
                    println!();
                    println!("Fingerprint: {}", fp);
                }
            }

            Ok(())
        })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ssh-key upload
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker ssh-key upload --server-id <ID> --public-key <FILE> --private-key <FILE>`
///
/// Uploads an existing SSH key pair to Vault for a server.
pub struct SshKeyUploadCommand {
    pub server_id: i32,
    pub public_key: PathBuf,
    pub private_key: PathBuf,
}

impl SshKeyUploadCommand {
    pub fn new(server_id: i32, public_key: PathBuf, private_key: PathBuf) -> Self {
        Self {
            server_id,
            public_key,
            private_key,
        }
    }
}

impl CallableTrait for SshKeyUploadCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let server_id = self.server_id;
        let pub_path = self.public_key.clone();
        let priv_path = self.private_key.clone();

        // Read key files
        let public_key = std::fs::read_to_string(&pub_path)
            .map_err(|e| CliError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read public key {}: {}", pub_path.display(), e),
            )))?;
        let private_key = std::fs::read_to_string(&priv_path)
            .map_err(|e| CliError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read private key {}: {}", priv_path.display(), e),
            )))?;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("ssh-key upload")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);
            let server = client
                .upload_ssh_key(server_id, public_key.trim(), private_key.trim())
                .await?;

            println!("✓ SSH key uploaded for server {}", server_id);
            println!("  Key status: {}", server.key_status);
            if let Some(name) = &server.name {
                println!("  Server: {}", name);
            }
            if let Some(ip) = &server.srv_ip {
                println!("  IP: {}", ip);
            }

            Ok(())
        })
    }
}
