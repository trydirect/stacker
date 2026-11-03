//! SSH client for remote server validation
//!
//! Uses russh to connect to servers and execute system check commands.

use base64::{engine::general_purpose, Engine as _};
use russh::client::{Config, Handle};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::PrivateKey;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

/// Result of a full system check via SSH
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCheckResult {
    /// SSH connection was successful
    pub connected: bool,
    /// SSH authentication was successful
    pub authenticated: bool,
    /// Username from whoami
    pub username: Option<String>,
    /// Total disk space in GB
    pub disk_total_gb: Option<f64>,
    /// Available disk space in GB
    pub disk_available_gb: Option<f64>,
    /// Disk usage percentage
    pub disk_usage_percent: Option<f64>,
    /// Docker is installed
    pub docker_installed: bool,
    /// Docker version string
    pub docker_version: Option<String>,
    /// OS name (from /etc/os-release)
    pub os_name: Option<String>,
    /// OS version
    pub os_version: Option<String>,
    /// Total memory in MB
    pub memory_total_mb: Option<u64>,
    /// Available memory in MB
    pub memory_available_mb: Option<u64>,
    /// Error message if validation failed
    pub error: Option<String>,
    /// Host key fingerprint the server presented, in OpenSSH form
    /// ("SHA256:base64"). Set whenever the handshake got far enough to see it.
    pub host_key_fingerprint: Option<String>,
    /// The server presented a host key that differs from the pinned one, so the
    /// connection was refused before authentication.
    pub host_key_mismatch: bool,
}

impl Default for SystemCheckResult {
    fn default() -> Self {
        Self {
            connected: false,
            authenticated: false,
            username: None,
            disk_total_gb: None,
            disk_available_gb: None,
            disk_usage_percent: None,
            docker_installed: false,
            docker_version: None,
            os_name: None,
            os_version: None,
            memory_total_mb: None,
            memory_available_mb: None,
            error: None,
            host_key_fingerprint: None,
            host_key_mismatch: false,
        }
    }
}

impl SystemCheckResult {
    /// Check if the system meets minimum requirements
    pub fn meets_requirements(&self) -> bool {
        self.connected
            && self.authenticated
            && self.docker_installed
            && self.disk_available_gb.map_or(false, |gb| gb >= 5.0)
    }

    /// Generate a human-readable summary
    pub fn summary(&self) -> String {
        if self.host_key_mismatch {
            return "Host key mismatch".to_string();
        }
        if !self.connected {
            return "Connection failed".to_string();
        }
        if !self.authenticated {
            return "Authentication failed".to_string();
        }

        let mut parts = vec![];

        if let Some(os) = &self.os_name {
            if let Some(ver) = &self.os_version {
                parts.push(format!("{} {}", os, ver));
            } else {
                parts.push(os.clone());
            }
        }

        if let Some(disk) = self.disk_available_gb {
            parts.push(format!("{:.1}GB available", disk));
        }

        if self.docker_installed {
            if let Some(ver) = &self.docker_version {
                parts.push(format!("Docker {}", ver));
            } else {
                parts.push("Docker installed".to_string());
            }
        } else {
            parts.push("Docker NOT installed".to_string());
        }

        if parts.is_empty() {
            "Connected".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Opaque wrapper around an authenticated SSH handle.
///
/// Returned by [`open_ssh`] and consumed by [`exec_remote`] and
/// [`disconnect_ssh`]. Keeps `ClientHandler` private to this module.
pub struct SshSession(Handle<ClientHandler>);

/// How a connection decides whether to trust the key the server presents.
///
/// russh 0.61 does not verify host keys on its own: the default
/// `check_server_key` in the trait returns `Ok(true)`, so the check has to be
/// ours. Without it anything answering on the server's address can impersonate
/// the customer's server and receive an authorized-key write or our commands.
#[derive(Debug, Clone)]
pub enum HostKeyPolicy {
    /// Compare against a fingerprint the caller stored for this server.
    ///
    /// `None` accepts the first key seen and records it; the caller reads the
    /// observed fingerprint back and stores it. This is the platform's mode,
    /// backed by `server.host_key_fingerprint`.
    Pin(Option<String>),
    /// Verify against an OpenSSH `known_hosts` file, appending the key on
    /// first use.
    ///
    /// This is the CLI's mode: it has no database, but it runs as a real user
    /// who already has a `known_hosts`. Using that file means `stacker` and
    /// `ssh` agree on the same pins, and a stale pin is cleared the way users
    /// already know: `ssh-keygen -R <host>`.
    KnownHosts {
        host: String,
        port: u16,
        path: PathBuf,
    },
}

impl HostKeyPolicy {
    /// Accept the first key seen and report it. For a host with no stored
    /// identity to compare against.
    pub fn trust_first_use() -> Self {
        Self::Pin(None)
    }

    /// Verify against the user's own `~/.ssh/known_hosts`.
    ///
    /// Falls back to `Pin(None)` when there is no home directory to read, so a
    /// CLI in an environment without one still works rather than failing to
    /// connect at all.
    pub fn user_known_hosts(host: &str, port: u16) -> Self {
        match home_ssh_known_hosts() {
            Some(path) => Self::KnownHosts {
                host: host.to_string(),
                port,
                path,
            },
            None => {
                tracing::warn!(
                    "No home directory for a known_hosts file; not verifying the host key of {}",
                    host
                );
                Self::Pin(None)
            }
        }
    }
}

impl HostKeyPolicy {
    /// Decide on a presented key, returning the reason when refusing.
    ///
    /// Exposed so the console path, which builds its own russh handler, applies
    /// the same rules as the platform path instead of a second copy of them.
    pub fn verify(&self, server_public_key: &russh::keys::PublicKey) -> Result<(), String> {
        let fingerprint = fingerprint_of(server_public_key);
        verify_host_key(self, server_public_key, &fingerprint)
    }
}

/// `~/.ssh/known_hosts`, the file `ssh` itself uses.
fn home_ssh_known_hosts() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".ssh").join("known_hosts"))
}

/// What the handler saw and decided, read back by the caller after the
/// handshake (russh owns the handler itself).
#[derive(Debug, Default, Clone)]
struct HostKeyObservation {
    /// The fingerprint the server presented, once the handshake got that far.
    fingerprint: Option<String>,
    /// Set when the handler refused the key, carrying the reason to report.
    /// russh reports a refusal as "Unknown server key", which `check_server`'s
    /// classifier folds into "Authentication failed" and hides.
    refusal: Option<String>,
}

type ObservedHostKey = Arc<Mutex<HostKeyObservation>>;

/// Render a host key the way OpenSSH does: `SHA256:` plus unpadded base64.
fn fingerprint_of(server_public_key: &russh::keys::PublicKey) -> String {
    server_public_key
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string()
}

fn observation(slot: &ObservedHostKey) -> HostKeyObservation {
    slot.lock().map(|guard| guard.clone()).unwrap_or_default()
}

/// Decide on a presented key, and say why when refusing.
///
/// Split out from the handler so it can be tested without a live server.
fn verify_host_key(
    policy: &HostKeyPolicy,
    server_public_key: &russh::keys::PublicKey,
    fingerprint: &str,
) -> Result<(), String> {
    match policy {
        // Trust on first use: nothing stored yet, so record and accept. The
        // caller pins what we recorded once the connection succeeds.
        HostKeyPolicy::Pin(None) => Ok(()),
        HostKeyPolicy::Pin(Some(pinned)) => {
            if pinned == fingerprint {
                Ok(())
            } else {
                Err(format!(
                    "Host key mismatch: server presented {fingerprint}, but {pinned} is pinned \
                     for it. Either the server was rebuilt (clear the pin to re-trust it) or the \
                     connection is being intercepted."
                ))
            }
        }
        HostKeyPolicy::KnownHosts { host, port, path } => {
            // Compare and store the key without its comment. russh's
            // known_hosts check compares whole `PublicKey` values, and
            // `PublicKey`'s equality includes the comment, so a key carrying
            // one never matches the comment-free line that `learn` wrote. A
            // handshake key normally has no comment, but a server that sent one
            // would otherwise be reported as a host key mismatch and lock the
            // user out of their own machine.
            let mut key = server_public_key.clone();
            key.set_comment("");
            let server_public_key = &key;

            match russh::keys::known_hosts::check_known_hosts_path(
                host,
                *port,
                server_public_key,
                path,
            ) {
                // Recorded already, and it matches.
                Ok(true) => Ok(()),
                // Not known yet: record it and accept, like ssh on first
                // connection. A failure to write is not fatal; it only means
                // the next connection asks the same question.
                Ok(false) => {
                    if let Err(e) = russh::keys::known_hosts::learn_known_hosts_path(
                        host,
                        *port,
                        server_public_key,
                        path,
                    ) {
                        tracing::warn!(
                            "Could not record host key {} for {} in {}: {}",
                            fingerprint,
                            host,
                            path.display(),
                            e
                        );
                    } else {
                        tracing::info!(
                            "Recorded host key {} for {} in {}",
                            fingerprint,
                            host,
                            path.display()
                        );
                    }
                    Ok(())
                }
                Err(russh::keys::Error::KeyChanged { line }) => Err(format!(
                    "Host key mismatch: {host} presented {fingerprint}, which differs from the \
                     key recorded on line {line} of {}. Either the server was rebuilt (run \
                     `ssh-keygen -R {host}` to forget the old key) or the connection is being \
                     intercepted.",
                    path.display()
                )),
                // Fail closed: an unreadable known_hosts is not a reason to
                // trust an unverified key.
                Err(e) => Err(format!(
                    "Could not verify the host key of {host} against {}: {e}",
                    path.display()
                )),
            }
        }
    }
}

/// SSH client handler for russh.
struct ClientHandler {
    policy: Arc<HostKeyPolicy>,
    /// Filled in with what the server presented and what we decided.
    observed_host_key: ObservedHostKey,
}

impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = fingerprint_of(server_public_key);
        let verdict = verify_host_key(&self.policy, server_public_key, &fingerprint);

        if let Ok(mut slot) = self.observed_host_key.lock() {
            slot.fingerprint = Some(fingerprint);
            slot.refusal = verdict.as_ref().err().cloned();
        }

        Ok(verdict.is_ok())
    }
}

/// Perform a full system check via SSH
///
/// Connects to the server, authenticates with the provided private key,
/// and runs diagnostic commands to gather system information.
pub async fn check_server(
    host: &str,
    port: u16,
    username: &str,
    private_key_pem: &str,
    connection_timeout: Duration,
    host_key_policy: &HostKeyPolicy,
) -> SystemCheckResult {
    let mut result = SystemCheckResult::default();

    // Parse the private key
    let key = match parse_private_key(private_key_pem) {
        Ok(k) => k,
        Err(e) => {
            tracing::error!("Failed to parse SSH private key: {}", e);
            result.error = Some(format!("Invalid SSH key: {}", e));
            return result;
        }
    };

    // Build SSH config
    let config = Arc::new(Config {
        ..Default::default()
    });

    // Connect with timeout
    let addr = format!("{}:{}", host, port);
    tracing::info!("Connecting to {} as {}", addr, username);

    let observed = ObservedHostKey::default();
    let connection_result = timeout(
        connection_timeout,
        connect_and_auth(
            config,
            &addr,
            username,
            key,
            Arc::new(host_key_policy.clone()),
            observed.clone(),
        ),
    )
    .await;

    let seen = observation(&observed);
    result.host_key_fingerprint = seen.fingerprint.clone();

    match connection_result {
        Ok(Ok(handle)) => {
            result.connected = true;
            result.authenticated = true;
            tracing::info!("SSH connection established successfully");

            // Run system checks
            run_system_checks(&mut result, handle).await;
        }
        Ok(Err(e)) => {
            tracing::warn!("SSH connection/auth failed: {}", e);
            // A refused host key must be reported before the classifier below:
            // russh calls it "Unknown server key", which contains "key" and
            // would be reported as an authentication failure.
            if let Some(refusal) = seen.refusal {
                tracing::error!("SSH host key refused for {}: {}", addr, refusal);
                result.connected = true;
                result.host_key_mismatch = true;
                result.error = Some(refusal);
                return result;
            }
            let error_str = e.to_string().to_lowercase();
            if error_str.contains("auth")
                || error_str.contains("key")
                || error_str.contains("permission")
            {
                result.connected = true;
                result.error = Some(format!("Authentication failed: {}", e));
            } else {
                result.error = Some(format!("Connection failed: {}", e));
            }
        }
        Err(_) => {
            tracing::warn!("SSH connection timed out after {:?}", connection_timeout);
            result.error = Some(format!(
                "Connection timed out after {} seconds",
                connection_timeout.as_secs()
            ));
        }
    }

    result
}

/// Authorize an OpenSSH public key on the remote server using an accepted private key.
///
/// `expected_host_key` is the server's pinned fingerprint, or `None` to trust
/// whatever it presents this once. Returns the fingerprint that was observed so
/// the caller can pin it on first use.
pub async fn authorize_public_key(
    host: &str,
    port: u16,
    username: &str,
    private_key_pem: &str,
    public_key: &str,
    connection_timeout: Duration,
    host_key_policy: &HostKeyPolicy,
) -> Result<Option<String>, anyhow::Error> {
    let public_key = public_key.trim();
    if public_key.is_empty() {
        return Err(anyhow::anyhow!("Public key cannot be empty"));
    }

    let key = parse_private_key(private_key_pem)?;
    let config = Arc::new(Config {
        ..Default::default()
    });
    let addr = format!("{}:{}", host, port);

    let observed = ObservedHostKey::default();
    let connection_result = timeout(
        connection_timeout,
        connect_and_auth(
            config,
            &addr,
            username,
            key,
            Arc::new(host_key_policy.clone()),
            observed.clone(),
        ),
    )
    .await;

    let seen = observation(&observed);

    let handle = match connection_result {
        Ok(Ok(handle)) => handle,
        Ok(Err(error)) => {
            // Never let a refused host key surface as a generic auth failure:
            // writing an authorized key into an impostor is the whole risk here.
            if let Some(refusal) = seen.refusal {
                return Err(anyhow::anyhow!(refusal));
            }
            return Err(error);
        }
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Connection timed out after {} seconds",
                connection_timeout.as_secs()
            ))
        }
    };

    let encoded_key = general_purpose::STANDARD.encode(public_key.as_bytes());
    let command = format!(
        "set -eu; key=$(printf '%s' '{}' | base64 -d); \
         mkdir -p ~/.ssh; chmod 700 ~/.ssh; \
         touch ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys; \
         grep -qxF \"$key\" ~/.ssh/authorized_keys || printf '%s\\n' \"$key\" >> ~/.ssh/authorized_keys",
        encoded_key
    );

    let result = exec_command_checked(&handle, &command).await;
    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;

    result.map(|()| seen.fingerprint)
}

/// Parse a PEM-encoded private key (OpenSSH or traditional formats)
fn parse_private_key(pem: &str) -> Result<PrivateKey, anyhow::Error> {
    // russh-keys supports various formats including OpenSSH and traditional PEM
    let key = russh::keys::decode_secret_key(pem, None)?;
    Ok(key)
}

async fn exec_command_checked(
    handle: &Handle<ClientHandler>,
    command: &str,
) -> Result<(), anyhow::Error> {
    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, command).await?;

    let mut stderr = Vec::new();
    let mut exit_status = None;
    let timeout_duration = Duration::from_secs(10);

    let read_result = timeout(timeout_duration, async {
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::ExtendedData { data, ext: _ }) => {
                    stderr.extend_from_slice(&data);
                }
                Some(russh::ChannelMsg::ExitStatus {
                    exit_status: status,
                }) => {
                    exit_status = Some(status);
                }
                Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    })
    .await;

    let _ = channel.eof().await;
    let _ = channel.close().await;

    if read_result.is_err() {
        return Err(anyhow::anyhow!("Remote authorization command timed out"));
    }

    if exit_status.unwrap_or(0) != 0 {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "Remote authorization command failed".to_string()
        } else {
            format!("Remote authorization command failed: {}", stderr)
        };
        return Err(anyhow::anyhow!(message));
    }

    Ok(())
}

/// Connect and authenticate to the SSH server
async fn connect_and_auth(
    config: Arc<Config>,
    addr: &str,
    username: &str,
    key: PrivateKey,
    policy: Arc<HostKeyPolicy>,
    observed_host_key: ObservedHostKey,
) -> Result<Handle<ClientHandler>, anyhow::Error> {
    let handler = ClientHandler {
        policy,
        observed_host_key,
    };
    let mut handle = russh::client::connect(config, addr, handler).await?;

    // Authenticate with public key
    let auth_res = handle
        .authenticate_publickey(
            username,
            PrivateKeyWithHashAlg::new(
                Arc::new(key),
                handle.best_supported_rsa_hash().await?.flatten(),
            ),
        )
        .await?;

    if !auth_res.success() {
        return Err(anyhow::anyhow!("Public key authentication failed"));
    }

    Ok(handle)
}

/// Run system check commands and populate the result
async fn run_system_checks(result: &mut SystemCheckResult, handle: Handle<ClientHandler>) {
    // Check username
    if let Ok(output) = exec_command(&handle, "whoami").await {
        result.username = Some(output.trim().to_string());
    }

    // Check disk space (df -BG /)
    if let Ok(output) = exec_command(&handle, "df -BG / 2>/dev/null | tail -1").await {
        parse_disk_info(result, &output);
    }

    // Check Docker
    match exec_command(&handle, "docker --version 2>/dev/null").await {
        Ok(output) if !output.is_empty() && !output.contains("not found") => {
            result.docker_installed = true;
            // Extract version number (e.g., "Docker version 24.0.5, build ced0996")
            if let Some(version) = output
                .strip_prefix("Docker version ")
                .and_then(|s| s.split(',').next())
            {
                result.docker_version = Some(version.trim().to_string());
            }
        }
        _ => {
            result.docker_installed = false;
        }
    }

    // Check OS info
    if let Ok(output) = exec_command(&handle, "cat /etc/os-release 2>/dev/null").await {
        parse_os_info(result, &output);
    }

    // Check memory (free -m)
    if let Ok(output) = exec_command(&handle, "free -m 2>/dev/null | grep -i mem").await {
        parse_memory_info(result, &output);
    }
}

/// Execute a command on the remote server and return stdout
async fn exec_command(
    handle: &Handle<ClientHandler>,
    command: &str,
) -> Result<String, anyhow::Error> {
    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, command).await?;

    let mut output = Vec::new();
    let timeout_duration = Duration::from_secs(10);

    let read_result = timeout(timeout_duration, async {
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Data { data }) => {
                    output.extend_from_slice(&data);
                }
                Some(russh::ChannelMsg::ExtendedData { data, ext: _ }) => {
                    // stderr - ignore for now
                    let _ = data;
                }
                Some(russh::ChannelMsg::Eof) => break,
                Some(russh::ChannelMsg::ExitStatus { exit_status: _ }) => {}
                Some(russh::ChannelMsg::Close) => break,
                None => break,
                _ => {}
            }
        }
    })
    .await;

    if read_result.is_err() {
        tracing::warn!("Command '{}' timed out", command);
    }

    // Close the channel
    let _ = channel.eof().await;
    let _ = channel.close().await;

    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Run a command on an open [`SshSession`], returning `(stdout, stderr, exit_code)`.
///
/// Uses a configurable timeout so long-running commands (e.g. install scripts) don't
/// block indefinitely. Surfaces stderr so callers can surface errors to the user.
pub async fn exec_remote(
    session: &SshSession,
    command: &str,
    timeout_secs: u64,
) -> Result<(String, String, u32), anyhow::Error> {
    let mut channel = session.0.channel_open_session().await?;
    channel.exec(true, command).await?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code: u32 = 0;
    let timeout_duration = Duration::from_secs(timeout_secs);

    let read_result = timeout(timeout_duration, async {
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                }
                Some(russh::ChannelMsg::ExtendedData { data, ext: _ }) => {
                    stderr.extend_from_slice(&data);
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = exit_status;
                }
                Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    })
    .await;

    let _ = channel.eof().await;
    let _ = channel.close().await;

    if read_result.is_err() {
        return Err(anyhow::anyhow!(
            "Command timed out after {} seconds: {}",
            timeout_secs,
            command
        ));
    }

    Ok((
        String::from_utf8_lossy(&stdout).to_string(),
        String::from_utf8_lossy(&stderr).to_string(),
        exit_code,
    ))
}

/// Open an SSH connection and authenticate with a PEM private key.
///
/// Returns an [`SshSession`] that can be passed to [`exec_remote`] multiple times.
/// Call [`disconnect_ssh`] when done.
///
/// `expected_host_key` is the server's pinned fingerprint, or `None` to trust
/// whatever it presents this once.
pub async fn open_ssh(
    host: &str,
    port: u16,
    username: &str,
    private_key_pem: &str,
    connection_timeout: Duration,
    host_key_policy: &HostKeyPolicy,
) -> Result<SshSession, anyhow::Error> {
    let key = parse_private_key(private_key_pem)?;
    let config = Arc::new(Config::default());
    let addr = format!("{}:{}", host, port);

    let observed = ObservedHostKey::default();
    let connection_result = timeout(
        connection_timeout,
        connect_and_auth(
            config,
            &addr,
            username,
            key,
            Arc::new(host_key_policy.clone()),
            observed.clone(),
        ),
    )
    .await;

    match connection_result {
        Ok(Ok(handle)) => Ok(SshSession(handle)),
        Ok(Err(error)) => {
            if let Some(refusal) = observation(&observed).refusal {
                return Err(anyhow::anyhow!(refusal));
            }
            Err(error)
        }
        Err(_) => Err(anyhow::anyhow!(
            "SSH connection timed out after {} seconds",
            connection_timeout.as_secs()
        )),
    }
}

/// Gracefully disconnect an open [`SshSession`].
pub async fn disconnect_ssh(session: SshSession) {
    let _ = session
        .0
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;
}

/// Parse disk info from df output
fn parse_disk_info(result: &mut SystemCheckResult, output: &str) {
    // df -BG output: "Filesystem     1G-blocks  Used Available Use% Mounted on"
    // Example line: "/dev/sda1         50G    20G       28G  42% /"
    let parts: Vec<&str> = output.split_whitespace().collect();
    if parts.len() >= 4 {
        // Parse total (index 1)
        if let Some(total) = parts
            .get(1)
            .and_then(|s| s.trim_end_matches('G').parse::<f64>().ok())
        {
            result.disk_total_gb = Some(total);
        }

        // Parse available (index 3)
        if let Some(avail) = parts
            .get(3)
            .and_then(|s| s.trim_end_matches('G').parse::<f64>().ok())
        {
            result.disk_available_gb = Some(avail);
        }

        // Parse usage percentage (index 4)
        if let Some(usage) = parts
            .get(4)
            .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
        {
            result.disk_usage_percent = Some(usage);
        }
    }
}

/// Parse OS info from /etc/os-release
fn parse_os_info(result: &mut SystemCheckResult, output: &str) {
    for line in output.lines() {
        if line.starts_with("NAME=") {
            result.os_name = Some(
                line.trim_start_matches("NAME=")
                    .trim_matches('"')
                    .to_string(),
            );
        } else if line.starts_with("VERSION=") {
            result.os_version = Some(
                line.trim_start_matches("VERSION=")
                    .trim_matches('"')
                    .to_string(),
            );
        } else if line.starts_with("VERSION_ID=") && result.os_version.is_none() {
            result.os_version = Some(
                line.trim_start_matches("VERSION_ID=")
                    .trim_matches('"')
                    .to_string(),
            );
        }
    }
}

/// Parse memory info from free -m output
fn parse_memory_info(result: &mut SystemCheckResult, output: &str) {
    // free -m | grep Mem output: "Mem:          15883        5234        8234         123        2414       10315"
    let parts: Vec<&str> = output.split_whitespace().collect();
    if parts.len() >= 4 {
        // Total memory (index 1)
        if let Some(total) = parts.get(1).and_then(|s| s.parse::<u64>().ok()) {
            result.memory_total_mb = Some(total);
        }

        // Available memory (index 6 in newer free, or calculate from free + buffers/cache)
        // For simplicity, use the "free" column (index 3) + buffers/cache (index 5) if available
        if let Some(avail) = parts.get(6).and_then(|s| s.parse::<u64>().ok()) {
            result.memory_available_mb = Some(avail);
        } else if let Some(free) = parts.get(3).and_then(|s| s.parse::<u64>().ok()) {
            // Fallback to free column
            result.memory_available_mb = Some(free);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::client::Handler as _;

    #[test]
    fn test_parse_disk_info() {
        let mut result = SystemCheckResult::default();
        parse_disk_info(&mut result, "/dev/sda1         50G    20G       28G  42% /");

        assert_eq!(result.disk_total_gb, Some(50.0));
        assert_eq!(result.disk_available_gb, Some(28.0));
        assert_eq!(result.disk_usage_percent, Some(42.0));
    }

    #[test]
    fn test_parse_os_info() {
        let mut result = SystemCheckResult::default();
        let os_release = r#"NAME="Ubuntu"
VERSION="22.04.3 LTS (Jammy Jellyfish)"
ID=ubuntu
VERSION_ID="22.04"
"#;
        parse_os_info(&mut result, os_release);

        assert_eq!(result.os_name, Some("Ubuntu".to_string()));
        assert_eq!(
            result.os_version,
            Some("22.04.3 LTS (Jammy Jellyfish)".to_string())
        );
    }

    #[test]
    fn test_parse_memory_info() {
        let mut result = SystemCheckResult::default();
        parse_memory_info(
            &mut result,
            "Mem:          15883        5234        8234         123        2414       10315",
        );

        assert_eq!(result.memory_total_mb, Some(15883));
        assert_eq!(result.memory_available_mb, Some(10315));
    }

    // Two real ed25519 host keys with the fingerprints `ssh-keygen -lf` prints
    // for them. Fixtures rather than generated keys: they also prove our
    // fingerprint matches OpenSSH's own output byte for byte.
    const HOST_KEY_A: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJ9VHNAX4w64aFbnFqInx3XrNBEO2g1SI204ucs5CPFT stacker-test-host-1";
    const HOST_KEY_A_FINGERPRINT: &str = "SHA256:+Was73QCQhCKk/fSB8vCxnlskm5AVrNuQ8w+vRxSC+U";
    const HOST_KEY_B: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPy1ljHDJhzOuapR6OHOl631lPexUnQ23//RekOGy6Jl stacker-test-host-2";
    const HOST_KEY_B_FINGERPRINT: &str = "SHA256:tZ8fKhLGQRHUhLtTN2m8uAs86AyvgxagUui3pQxrbgg";

    fn host_key(openssh: &str) -> russh::keys::PublicKey {
        russh::keys::PublicKey::from_openssh(openssh).expect("parse test host key")
    }

    /// Our fingerprint must be the one an operator sees from `ssh-keygen -lf`,
    /// otherwise a pin can never be compared against anything by hand.
    #[test]
    fn fingerprint_matches_openssh() {
        assert_eq!(
            fingerprint_of(&host_key(HOST_KEY_A)),
            HOST_KEY_A_FINGERPRINT
        );
        assert_eq!(
            fingerprint_of(&host_key(HOST_KEY_B)),
            HOST_KEY_B_FINGERPRINT
        );
    }

    // ── Pin mode (the platform: server.host_key_fingerprint) ──────────────

    /// A key the handler has never been told about is recorded and accepted, so
    /// the caller can pin it (trust on first use).
    #[tokio::test]
    async fn unpinned_host_key_is_accepted_and_recorded() {
        let observed = ObservedHostKey::default();
        let mut handler = ClientHandler {
            policy: Arc::new(HostKeyPolicy::trust_first_use()),
            observed_host_key: observed.clone(),
        };

        let accepted = handler
            .check_server_key(&host_key(HOST_KEY_A))
            .await
            .expect("check_server_key");

        assert!(accepted, "an unpinned host key should be accepted once");
        let seen = observation(&observed);
        assert_eq!(
            seen.fingerprint.as_deref(),
            Some(HOST_KEY_A_FINGERPRINT),
            "the observed key must be recorded so the caller can pin it"
        );
        assert!(seen.refusal.is_none());
    }

    /// The pinned key being presented again is accepted.
    #[tokio::test]
    async fn matching_host_key_is_accepted() {
        let observed = ObservedHostKey::default();
        let mut handler = ClientHandler {
            policy: Arc::new(HostKeyPolicy::Pin(Some(HOST_KEY_A_FINGERPRINT.to_string()))),
            observed_host_key: observed.clone(),
        };

        assert!(handler
            .check_server_key(&host_key(HOST_KEY_A))
            .await
            .expect("check_server_key"));
        assert!(observation(&observed).refusal.is_none());
    }

    /// The regression that matters: a different key on a pinned server is
    /// refused, so nothing is written to it and no command runs on it.
    /// Returning `Ok(true)` from `check_server_key` must fail this test.
    #[tokio::test]
    async fn different_host_key_on_a_pinned_server_is_refused() {
        let observed = ObservedHostKey::default();
        let mut handler = ClientHandler {
            policy: Arc::new(HostKeyPolicy::Pin(Some(HOST_KEY_A_FINGERPRINT.to_string()))),
            observed_host_key: observed.clone(),
        };

        let accepted = handler
            .check_server_key(&host_key(HOST_KEY_B))
            .await
            .expect("check_server_key");

        assert!(
            !accepted,
            "a host key that differs from the pin must be refused"
        );
        let seen = observation(&observed);
        assert_eq!(
            seen.fingerprint.as_deref(),
            Some(HOST_KEY_B_FINGERPRINT),
            "what the impostor presented must still be recorded"
        );
        // The reason has to name both keys, otherwise an operator cannot tell a
        // rebuilt server from an interception.
        let refusal = seen.refusal.expect("a refusal reason");
        assert!(refusal.contains(HOST_KEY_A_FINGERPRINT), "{refusal}");
        assert!(refusal.contains(HOST_KEY_B_FINGERPRINT), "{refusal}");
    }

    // ── known_hosts mode (the CLI: the user's own ~/.ssh/known_hosts) ──────

    fn temp_known_hosts(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "stacker-known-hosts-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    /// First connection: the host is unknown, so the key is accepted and
    /// appended, exactly as `ssh` does.
    #[test]
    fn unknown_host_is_learned_into_known_hosts() {
        let path = temp_known_hosts("learn");
        let policy = HostKeyPolicy::KnownHosts {
            host: "server.example".to_string(),
            port: 22,
            path: path.clone(),
        };

        assert!(policy.verify(&host_key(HOST_KEY_A)).is_ok());
        let recorded =
            std::fs::read_to_string(&path).expect("known_hosts should have been written");
        assert!(
            recorded.contains("server.example"),
            "the host should be recorded: {recorded}"
        );

        // Second connection with the same key is accepted against the record.
        assert!(policy.verify(&host_key(HOST_KEY_A)).is_ok());

        std::fs::remove_file(&path).ok();
    }

    /// A host that is recorded but presents a different key is refused, and the
    /// reason tells the user how to forget the old one.
    #[test]
    fn changed_host_key_is_refused_against_known_hosts() {
        let path = temp_known_hosts("changed");
        let policy = HostKeyPolicy::KnownHosts {
            host: "server.example".to_string(),
            port: 22,
            path: path.clone(),
        };

        policy.verify(&host_key(HOST_KEY_A)).expect("first use");

        let refusal = policy
            .verify(&host_key(HOST_KEY_B))
            .expect_err("a changed host key must be refused");

        assert!(refusal.contains(HOST_KEY_B_FINGERPRINT), "{refusal}");
        assert!(
            refusal.contains("ssh-keygen -R"),
            "the reason must say how to forget the old key: {refusal}"
        );

        std::fs::remove_file(&path).ok();
    }

    /// A non-default port is recorded as `[host]:port`, so two services on one
    /// address do not overwrite each other's entry.
    #[test]
    fn known_hosts_entries_are_per_port() {
        let path = temp_known_hosts("ports");
        let on_2222 = HostKeyPolicy::KnownHosts {
            host: "server.example".to_string(),
            port: 2222,
            path: path.clone(),
        };
        let on_22 = HostKeyPolicy::KnownHosts {
            host: "server.example".to_string(),
            port: 22,
            path: path.clone(),
        };

        on_2222.verify(&host_key(HOST_KEY_A)).expect("learn 2222");
        // Port 22 has no record yet, so a different key there is first use and
        // not a conflict with the entry for 2222.
        on_22.verify(&host_key(HOST_KEY_B)).expect("learn 22");
        // Each port now holds its own key.
        on_2222.verify(&host_key(HOST_KEY_A)).expect("2222 matches");
        on_22.verify(&host_key(HOST_KEY_B)).expect("22 matches");
        assert!(on_2222.verify(&host_key(HOST_KEY_B)).is_err());

        std::fs::remove_file(&path).ok();
    }

    /// A key that carries a comment must still match the comment-free line
    /// that `learn` wrote.
    ///
    /// russh compares whole `PublicKey` values and `PublicKey`'s equality
    /// includes the comment, so without normalizing it a commented key reads as
    /// a host key mismatch and locks the user out of their own server. The
    /// fixtures above carry comments, which is what caught this.
    #[test]
    fn a_commented_host_key_is_not_a_mismatch() {
        let path = temp_known_hosts("comment");
        let policy = HostKeyPolicy::KnownHosts {
            host: "server.example".to_string(),
            port: 22,
            path: path.clone(),
        };

        let commented = host_key(HOST_KEY_A);
        assert!(
            !commented.comment().is_empty(),
            "this test needs a key with a comment to be meaningful"
        );

        policy.verify(&commented).expect("first use");
        policy
            .verify(&commented)
            .expect("the same key must match the line just written for it");

        std::fs::remove_file(&path).ok();
    }

    /// A mismatch must not be reported as an auth failure: russh calls it
    /// "Unknown server key", which the classifier in `check_server` folds into
    /// "Authentication failed".
    #[test]
    fn summary_reports_a_host_key_mismatch() {
        let result = SystemCheckResult {
            connected: true,
            host_key_mismatch: true,
            ..Default::default()
        };

        assert_eq!(result.summary(), "Host key mismatch");
        assert!(!result.meets_requirements());
    }

    #[test]
    fn test_summary() {
        let mut result = SystemCheckResult::default();
        assert_eq!(result.summary(), "Connection failed");

        result.connected = true;
        assert_eq!(result.summary(), "Authentication failed");

        result.authenticated = true;
        result.os_name = Some("Ubuntu".to_string());
        result.os_version = Some("22.04".to_string());
        result.disk_available_gb = Some(50.0);
        result.docker_installed = true;
        result.docker_version = Some("24.0.5".to_string());

        assert_eq!(
            result.summary(),
            "Ubuntu 22.04, 50.0GB available, Docker 24.0.5"
        );
    }
}
