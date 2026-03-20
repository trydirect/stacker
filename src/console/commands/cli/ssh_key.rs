//! SSH key management commands — generate, show (read), upload, and inject keys.
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ssh-key inject
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker ssh-key inject --server-id <ID> --with-key <PATH> [--user <USER>] [--port <PORT>]`
///
/// Fetches the vault-stored public key for a server and injects it into the
/// server's `~/.ssh/authorized_keys` using a locally-available working private key.
///
/// Use this to repair a server whose `authorized_keys` doesn't contain the Stacker
/// vault key (e.g. after a fresh key generation that failed to inject automatically).
pub struct SshKeyInjectCommand {
    pub server_id: i32,
    /// Path to a local private key that already grants SSH access to the server.
    pub with_key: PathBuf,
    /// SSH user (default: root)
    pub user: Option<String>,
    /// SSH port override (default: server's stored ssh_port or 22)
    pub port: Option<u16>,
}

impl SshKeyInjectCommand {
    pub fn new(
        server_id: i32,
        with_key: PathBuf,
        user: Option<String>,
        port: Option<u16>,
    ) -> Self {
        Self { server_id, with_key, user, port }
    }
}

impl CallableTrait for SshKeyInjectCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let server_id = self.server_id;
        let key_path = self.with_key.clone();
        let override_user = self.user.clone();
        let override_port = self.port;

        // Read the local working private key
        let local_private_key = std::fs::read_to_string(&key_path)
            .map_err(|e| CliError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read key file {}: {}", key_path.display(), e),
            )))?;

        let cred_manager = CredentialsManager::with_default_store();
        let creds = cred_manager.require_valid_token("ssh-key inject")?;
        let base_url = stacker_client::DEFAULT_STACKER_URL.to_string();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| CliError::ConfigValidation(format!("Failed to create async runtime: {}", e)))?;

        rt.block_on(async {
            let client = StackerClient::new(&base_url, &creds.access_token);

            // Fetch server info to get IP, port, and user
            let servers = client.list_servers().await?;
            let server_info = servers
                .into_iter()
                .find(|s| s.id == server_id)
                .ok_or_else(|| CliError::ConfigValidation(
                    format!("Server {} not found", server_id)
                ))?;

            let host = server_info
                .srv_ip
                .as_deref()
                .filter(|ip| !ip.is_empty())
                .ok_or_else(|| CliError::ConfigValidation(
                    format!("Server {} has no IP address — deploy it first", server_id)
                ))?
                .to_string();

            let port = override_port
                .unwrap_or_else(|| server_info.ssh_port.unwrap_or(22) as u16);
            let user = override_user
                .or_else(|| server_info.ssh_user.clone())
                .unwrap_or_else(|| "root".to_string());

            // Fetch the vault public key
            let key_resp = client.get_ssh_public_key(server_id).await?;
            let vault_public_key = key_resp.public_key.trim().to_string();

            println!("Server:     {} (ID {})", host, server_id);
            println!("SSH user:   {}  port: {}", user, port);
            println!("Vault key:  {}", &vault_public_key[..vault_public_key.len().min(60)]);
            println!();
            println!("Connecting to inject key into authorized_keys…");

            inject_key_via_ssh(&host, port, &user, local_private_key.trim(), &vault_public_key).await
        })
    }
}

/// SSH into the server using `local_private_key` and append `vault_public_key`
/// to `~/.ssh/authorized_keys` if it is not already present.
async fn inject_key_via_ssh(
    host: &str,
    port: u16,
    username: &str,
    local_private_key: &str,
    vault_public_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use std::time::Duration;
    use russh::client::{Config, Handle};

    struct AcceptAllKeys;

    impl russh::client::Handler for AcceptAllKeys {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            _server_public_key: &russh::keys::PublicKey,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    let key = russh::keys::decode_secret_key(local_private_key, None)
        .map_err(|e| CliError::ConfigValidation(format!("Invalid private key: {}", e)))?;

    let config = Arc::new(Config {
        ..Default::default()
    });

    let addr = format!("{}:{}", host, port);
    let mut handle: Handle<AcceptAllKeys> =
        tokio::time::timeout(Duration::from_secs(4), russh::client::connect(config, addr, AcceptAllKeys))
            .await
            .map_err(|_| CliError::ConfigValidation(format!("Connection to {}:{} timed out", host, port)))?
            .map_err(|e| CliError::ConfigValidation(format!("Connection failed: {}", e)))?;

    let auth_res = handle
        .authenticate_publickey(
            username,
            russh::keys::key::PrivateKeyWithHashAlg::new(
                Arc::new(key),
                handle.best_supported_rsa_hash().await
                    .map_err(|e| CliError::ConfigValidation(format!("RSA hash negotiation failed: {}", e)))?
                    .flatten(),
            ),
        )
        .await
        .map_err(|e| CliError::ConfigValidation(format!("Authentication error: {}", e)))?;

    if !auth_res.success() {
        return Err(Box::new(CliError::ConfigValidation(
            "Authentication failed — the provided key is not accepted by the server".to_string(),
        )));
    }

    // Idempotent inject: add key only if not already present
    let safe_key = vault_public_key.replace('\'', r"'\''");
    let cmd = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && \
         grep -qxF '{}' ~/.ssh/authorized_keys || echo '{}' >> ~/.ssh/authorized_keys",
        safe_key, safe_key
    );

    let mut channel = handle.channel_open_session().await
        .map_err(|e| CliError::ConfigValidation(format!("Failed to open SSH channel: {}", e)))?;
    channel.exec(true, cmd).await
        .map_err(|e| CliError::ConfigValidation(format!("Failed to exec command: {}", e)))?;

    // Drain channel output
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
            Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                if exit_status != 0 {
                    return Err(Box::new(CliError::ConfigValidation(
                        format!("Remote command exited with status {}", exit_status),
                    )));
                }
                break;
            }
            _ => {}
        }
    }

    let _ = channel.eof().await;
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "English").await;

    println!("✓ Vault public key injected into {}@{}:{} authorized_keys", username, host, port);
    println!();
    println!("You can now run:  stacker deploy");

    Ok(())
}
