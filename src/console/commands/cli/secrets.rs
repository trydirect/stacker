//! Secrets / env management commands.
//!
//! Reads and writes a `.env` file (defaults to the path specified by
//! `env_file` in `stacker.yml`, falling back to `.env`).
//!
//! ```text
//! stacker secrets set   KEY=VALUE [--file .env]
//! stacker secrets get   KEY       [--file .env] [--show]
//! stacker secrets list            [--file .env] [--show]
//! stacker secrets delete KEY      [--file .env]
//! stacker secrets validate        [--file stacker.yml]
//! ```

use std::fmt;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;
use crate::cli::stacker_client::{ProjectAppInfo, ProjectInfo, RemoteSecretMetadataInfo};
use crate::console::commands::CallableTrait;
use clap::ValueEnum;

const DEFAULT_ENV_FILE: &str = ".env";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum RemoteSecretScope {
    Service,
    Server,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSecretTarget {
    scope: RemoteSecretScope,
    project: Option<String>,
    service: Option<String>,
    server_id: Option<i32>,
}

impl RemoteSecretTarget {
    fn new(
        scope: RemoteSecretScope,
        project: Option<String>,
        service: Option<String>,
        server_id: Option<i32>,
    ) -> Self {
        Self {
            scope,
            project,
            service,
            server_id,
        }
    }

    fn validate(&self) -> Result<(), CliError> {
        match self.scope {
            RemoteSecretScope::Service => {
                if self.project.as_deref().unwrap_or_default().is_empty() {
                    return Err(CliError::ConfigValidation(
                        "Service-scoped secrets require --project".to_string(),
                    ));
                }
                if self.service.as_deref().unwrap_or_default().is_empty() {
                    return Err(CliError::ConfigValidation(
                        "Service-scoped secrets require --service".to_string(),
                    ));
                }
                if self.server_id.is_some() {
                    return Err(CliError::ConfigValidation(
                        "Service-scoped secrets do not accept --server-id".to_string(),
                    ));
                }
            }
            RemoteSecretScope::Server => {
                if self.server_id.is_none() {
                    return Err(CliError::ConfigValidation(
                        "Server-scoped secrets require --server-id".to_string(),
                    ));
                }
                if self.project.is_some() || self.service.is_some() {
                    return Err(CliError::ConfigValidation(
                        "Server-scoped secrets do not accept --project or --service".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn project_ref(&self) -> Option<&str> {
        self.project.as_deref()
    }

    fn service_code(&self) -> Option<&str> {
        self.service.as_deref()
    }

    fn server_id(&self) -> Option<i32> {
        self.server_id
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RemoteSecretWriteOptions {
    name: String,
    target: RemoteSecretTarget,
    body: Option<String>,
    body_file: Option<String>,
}

impl fmt::Debug for RemoteSecretWriteOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteSecretWriteOptions")
            .field("name", &self.name)
            .field("target", &self.target)
            .field("body", &self.body.as_ref().map(|_| "[REDACTED]"))
            .field("body_file", &self.body_file)
            .finish()
    }
}

impl RemoteSecretWriteOptions {
    fn validate(&self) -> Result<(), CliError> {
        validate_secret_name(&self.name)?;
        self.target.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSecretReadOptions {
    name: String,
    target: RemoteSecretTarget,
    json: bool,
}

impl RemoteSecretReadOptions {
    fn validate(&self) -> Result<(), CliError> {
        validate_secret_name(&self.name)?;
        self.target.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteSecretListOptions {
    target: RemoteSecretTarget,
    json: bool,
}

impl RemoteSecretListOptions {
    fn validate(&self) -> Result<(), CliError> {
        self.target.validate()
    }
}

fn validate_secret_name(name: &str) -> Result<(), CliError> {
    let valid_key = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    if valid_key.is_match(name) {
        Ok(())
    } else {
        Err(CliError::ConfigValidation(format!(
            "Invalid key '{}': must match [A-Za-z_][A-Za-z0-9_]*",
            name
        )))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Shared helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Read `.env` file and return all lines (preserving comments/blanks).
fn read_env_lines(path: &Path) -> Result<Vec<String>, CliError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(content.lines().map(|l| l.to_string()).collect())
}

/// Parse a single `.env` line into `Some((key, value))` or `None` for
/// comment / blank / malformed lines.
fn parse_env_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if let Some(pos) = trimmed.find('=') {
        let key = trimmed[..pos].trim().to_string();
        let raw_val = trimmed[pos + 1..].trim();
        // Strip optional surrounding quotes
        let value = if (raw_val.starts_with('"') && raw_val.ends_with('"'))
            || (raw_val.starts_with('\'') && raw_val.ends_with('\''))
        {
            raw_val[1..raw_val.len() - 1].to_string()
        } else {
            raw_val.to_string()
        };
        Some((key, value))
    } else {
        None
    }
}

/// Write lines back to an `.env` file (creates it if absent).
fn write_env_lines(path: &Path, lines: &[String]) -> Result<(), CliError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Resolve the env file path: use explicit `--file`, otherwise look in
/// `stacker.yml`'s `env_file` field, otherwise default to `.env`.
fn validate_env_path(p: &str) -> Result<PathBuf, CliError> {
    let path = Path::new(p);
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(CliError::ConfigValidation(format!(
                "Path traversal ('..') is not allowed for --file: {}",
                p
            )));
        }
    }
    Ok(PathBuf::from(p))
}

fn resolve_env_path(explicit: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(p) = explicit {
        return validate_env_path(p);
    }
    // Try to read from stacker.yml
    if let Ok(content) = std::fs::read_to_string(DEFAULT_CONFIG_FILE) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("env_file:") {
                let val = trimmed["env_file:".len()..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !val.is_empty() {
                    return validate_env_path(val);
                }
            }
        }
    }
    Ok(PathBuf::from(DEFAULT_ENV_FILE))
}

fn resolve_remote_secret_value(options: &RemoteSecretWriteOptions) -> Result<String, CliError> {
    if let Some(body) = &options.body {
        return Ok(body.clone());
    }

    if let Some(body_file) = &options.body_file {
        return std::fs::read_to_string(body_file).map_err(CliError::from);
    }

    if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        if !buffer.is_empty() {
            return Ok(buffer);
        }
    }

    Err(CliError::ConfigValidation(
        "Remote secrets set requires --body, --body-file, or stdin input".to_string(),
    ))
}

fn remap_remote_secret_error(operation: &str, error: CliError) -> CliError {
    match error {
        CliError::DeployFailed { reason, .. } => CliError::FeatureFailed {
            feature: operation.to_string(),
            reason,
        },
        other => other,
    }
}

fn resolve_project(
    ctx: &CliRuntime,
    reference: &str,
    operation: &str,
) -> Result<ProjectInfo, CliError> {
    ctx.block_on(ctx.client.find_project(reference))
        .map_err(|error| remap_remote_secret_error(operation, error))?
        .ok_or_else(|| CliError::ConfigValidation(format!("Project '{}' was not found", reference)))
}

fn project_app_codes(project: &ProjectInfo) -> Vec<String> {
    let mut codes: Vec<String> = project
        .metadata
        .get("custom")
        .and_then(|custom| custom.get("web"))
        .and_then(|web| web.as_array())
        .map(|apps| {
            apps.iter()
                .filter_map(|app| app.get("code").and_then(|code| code.as_str()))
                .map(|code| code.to_string())
                .collect()
        })
        .unwrap_or_default();

    codes.sort();
    codes.dedup();
    codes
}

fn resolve_remote_service_code(project: &ProjectInfo, requested: &str) -> Result<String, CliError> {
    let available_codes = project_app_codes(project);
    if available_codes.is_empty() {
        return Ok(requested.to_string());
    }

    let requested_lower = requested.to_lowercase();
    if let Some(code) = available_codes
        .iter()
        .find(|code| code.to_lowercase() == requested_lower)
    {
        return Ok(code.clone());
    }

    Err(CliError::ConfigValidation(format!(
        "Service '{}' was not found in project '{}'. Available app codes: {}",
        requested,
        project.name,
        available_codes.join(", ")
    )))
}

fn print_remote_secret(secret: &RemoteSecretMetadataInfo, json: bool) -> Result<(), CliError> {
    if json {
        let rendered = serde_json::to_string_pretty(secret)
            .map_err(|error| CliError::ConfigValidation(error.to_string()))?;
        println!("{rendered}");
    } else {
        println!("Name: {}", secret.name);
        println!("Scope: {}", secret.scope);
        if let Some(project_id) = secret.project_id {
            println!("Project ID: {}", project_id);
        }
        if let Some(app_code) = &secret.app_code {
            println!("Service: {}", app_code);
        }
        if let Some(server_id) = secret.server_id {
            println!("Server ID: {}", server_id);
        }
        println!("Updated At: {}", secret.updated_at);
        println!("Updated By: {}", secret.updated_by);
        println!("Source: {}", secret.source);
        println!("Value: [REDACTED]");
    }

    Ok(())
}

fn print_remote_secret_list(
    secrets: &[RemoteSecretMetadataInfo],
    json: bool,
) -> Result<(), CliError> {
    if json {
        let rendered = serde_json::to_string_pretty(secrets)
            .map_err(|error| CliError::ConfigValidation(error.to_string()))?;
        println!("{rendered}");
        return Ok(());
    }

    if secrets.is_empty() {
        println!("(no remote secrets set)");
        return Ok(());
    }

    println!(
        "{:<32} {:<10} {:<20} {:<26}",
        "NAME", "SCOPE", "TARGET", "UPDATED"
    );
    println!("{}", "─".repeat(92));
    for secret in secrets {
        let target = if let Some(app_code) = &secret.app_code {
            format!("app:{app_code}")
        } else if let Some(server_id) = secret.server_id {
            format!("server:{server_id}")
        } else {
            "-".to_string()
        };
        println!(
            "{:<32} {:<10} {:<20} {:<26}",
            secret.name, secret.scope, target, secret.updated_at
        );
    }

    Ok(())
}

fn print_project_app_list(apps: &[ProjectAppInfo], json: bool) -> Result<(), CliError> {
    if json {
        let rendered = serde_json::to_string_pretty(apps)
            .map_err(|error| CliError::ConfigValidation(error.to_string()))?;
        println!("{rendered}");
        return Ok(());
    }

    if apps.is_empty() {
        println!("(no project apps found)");
        return Ok(());
    }

    println!(
        "{:<24} {:<24} {:<8} {:<12} {}",
        "CODE", "NAME", "ENABLED", "PARENT", "IMAGE"
    );
    println!("{}", "─".repeat(96));
    for app in apps {
        println!(
            "{:<24} {:<24} {:<8} {:<12} {}",
            app.code,
            app.name,
            if app.enabled { "yes" } else { "no" },
            app.parent_app_code.as_deref().unwrap_or("-"),
            app.image
        );
    }

    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets set
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets set KEY=VALUE [--file .env]`
pub struct SecretsSetCommand {
    mode: SecretsSetMode,
}

#[derive(Debug)]
enum SecretsSetMode {
    Local {
        key_value: String,
        file: Option<String>,
    },
    Remote(RemoteSecretWriteOptions),
}

impl SecretsSetCommand {
    pub fn new(key_value: String, file: Option<String>) -> Self {
        Self {
            mode: SecretsSetMode::Local { key_value, file },
        }
    }

    pub fn new_remote(
        name: String,
        scope: RemoteSecretScope,
        project: Option<String>,
        service: Option<String>,
        server_id: Option<i32>,
        body: Option<String>,
        body_file: Option<String>,
    ) -> Self {
        Self {
            mode: SecretsSetMode::Remote(RemoteSecretWriteOptions {
                name,
                target: RemoteSecretTarget::new(scope, project, service, server_id),
                body,
                body_file,
            }),
        }
    }

    fn call_local(key_value: &str, file: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let pos = key_value.find('=').ok_or_else(|| {
            CliError::ConfigValidation(
                "Expected KEY=VALUE format (e.g. DB_PASS=secret)".to_string(),
            )
        })?;
        let key = key_value[..pos].trim().to_string();
        let value = key_value[pos + 1..].to_string();

        validate_secret_name(&key)?;

        let env_path = resolve_env_path(file)?;
        let mut lines = read_env_lines(&env_path)?;

        let new_line = format!("{key}={value}");
        let mut found = false;
        for line in &mut lines {
            if let Some((k, _)) = parse_env_line(line) {
                if k == key {
                    *line = new_line.clone();
                    found = true;
                    break;
                }
            }
        }
        if !found {
            lines.push(new_line);
        }

        write_env_lines(&env_path, &lines)?;
        println!("✓ Set {key} in {}", env_path.display());
        Ok(())
    }

    fn call_remote(options: &RemoteSecretWriteOptions) -> Result<(), Box<dyn std::error::Error>> {
        options.validate()?;
        let value = resolve_remote_secret_value(options)?;
        let operation = "remote secrets set";
        let ctx = CliRuntime::new("remote secrets set")?;

        match options.target.scope {
            RemoteSecretScope::Service => {
                let project_ref = options.target.project_ref().ok_or_else(|| {
                    CliError::ConfigValidation(
                        "Service-scoped secrets require --project".to_string(),
                    )
                })?;
                let project = resolve_project(&ctx, project_ref, operation)?;
                let app_code = options.target.service_code().ok_or_else(|| {
                    CliError::ConfigValidation(
                        "Service-scoped secrets require --service".to_string(),
                    )
                })?;
                let app_code = resolve_remote_service_code(&project, app_code)?;
                let secret = ctx
                    .block_on(ctx.client.set_service_secret(
                        project.id,
                        &app_code,
                        &options.name,
                        &value,
                    ))
                    .map_err(|error| remap_remote_secret_error(operation, error))?;
                println!(
                    "✓ Saved {} secret {} for project {} service {}",
                    secret.scope, secret.name, project.id, app_code
                );
            }
            RemoteSecretScope::Server => {
                let server_id = options.target.server_id().ok_or_else(|| {
                    CliError::ConfigValidation(
                        "Server-scoped secrets require --server-id".to_string(),
                    )
                })?;
                let secret = ctx
                    .block_on(
                        ctx.client
                            .set_server_secret(server_id, &options.name, &value),
                    )
                    .map_err(|error| remap_remote_secret_error(operation, error))?;
                println!(
                    "✓ Saved {} secret {} for server {}",
                    secret.scope, secret.name, server_id
                );
            }
        }

        Ok(())
    }
}

impl CallableTrait for SecretsSetCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.mode {
            SecretsSetMode::Local { key_value, file } => {
                Self::call_local(key_value, file.as_deref())
            }
            SecretsSetMode::Remote(options) => Self::call_remote(options),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets get
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets get KEY [--file .env] [--show]`
pub struct SecretsGetCommand {
    mode: SecretsGetMode,
}

#[derive(Debug)]
enum SecretsGetMode {
    Local {
        key: String,
        file: Option<String>,
        show: bool,
    },
    Remote(RemoteSecretReadOptions),
}

impl SecretsGetCommand {
    pub fn new(key: String, file: Option<String>, show: bool) -> Self {
        Self {
            mode: SecretsGetMode::Local { key, file, show },
        }
    }

    pub fn new_remote(
        name: String,
        scope: RemoteSecretScope,
        project: Option<String>,
        service: Option<String>,
        server_id: Option<i32>,
        json: bool,
    ) -> Self {
        Self {
            mode: SecretsGetMode::Remote(RemoteSecretReadOptions {
                name,
                target: RemoteSecretTarget::new(scope, project, service, server_id),
                json,
            }),
        }
    }
}

impl CallableTrait for SecretsGetCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.mode {
            SecretsGetMode::Local { key, file, show } => {
                let env_path = resolve_env_path(file.as_deref())?;

                if !env_path.exists() {
                    return Err(Box::new(CliError::EnvFileNotFound { path: env_path }));
                }

                let lines = read_env_lines(&env_path)?;
                for line in &lines {
                    if let Some((k, v)) = parse_env_line(line) {
                        if k == *key {
                            if *show {
                                println!("{k}={v}");
                            } else {
                                println!("{k}=***");
                            }
                            return Ok(());
                        }
                    }
                }

                Err(Box::new(CliError::SecretKeyNotFound { key: key.clone() }))
            }
            SecretsGetMode::Remote(options) => {
                options.validate()?;
                let operation = "remote secrets get";
                let ctx = CliRuntime::new("remote secrets get")?;
                let secret = match options.target.scope {
                    RemoteSecretScope::Service => {
                        let project_ref = options.target.project_ref().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --project".to_string(),
                            )
                        })?;
                        let project = resolve_project(&ctx, project_ref, operation)?;
                        let app_code = options.target.service_code().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --service".to_string(),
                            )
                        })?;
                        let app_code = resolve_remote_service_code(&project, app_code)?;
                        ctx.block_on(ctx.client.get_service_secret_metadata(
                            project.id,
                            &app_code,
                            &options.name,
                        ))
                        .map_err(|error| remap_remote_secret_error(operation, error))?
                    }
                    RemoteSecretScope::Server => {
                        let server_id = options.target.server_id().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Server-scoped secrets require --server-id".to_string(),
                            )
                        })?;
                        ctx.block_on(
                            ctx.client
                                .get_server_secret_metadata(server_id, &options.name),
                        )
                        .map_err(|error| remap_remote_secret_error(operation, error))?
                    }
                }
                .ok_or_else(|| CliError::SecretKeyNotFound {
                    key: options.name.clone(),
                })?;

                print_remote_secret(&secret, options.json)?;
                Ok(())
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets list
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets list [--file .env] [--show]`
pub struct SecretsListCommand {
    mode: SecretsListMode,
}

#[derive(Debug)]
enum SecretsListMode {
    Local { file: Option<String>, show: bool },
    Remote(RemoteSecretListOptions),
}

impl SecretsListCommand {
    pub fn new(file: Option<String>, show: bool) -> Self {
        Self {
            mode: SecretsListMode::Local { file, show },
        }
    }

    pub fn new_remote(
        scope: RemoteSecretScope,
        project: Option<String>,
        service: Option<String>,
        server_id: Option<i32>,
        json: bool,
    ) -> Self {
        Self {
            mode: SecretsListMode::Remote(RemoteSecretListOptions {
                target: RemoteSecretTarget::new(scope, project, service, server_id),
                json,
            }),
        }
    }
}

impl CallableTrait for SecretsListCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.mode {
            SecretsListMode::Local { file, show } => {
                let env_path = resolve_env_path(file.as_deref())?;

                if !env_path.exists() {
                    eprintln!(
                        "No env file found at {}. Use `stacker secrets set KEY=VALUE` to create one.",
                        env_path.display()
                    );
                    return Ok(());
                }

                let lines = read_env_lines(&env_path)?;
                let mut count = 0;

                println!("Secrets in {}:", env_path.display());
                for line in &lines {
                    if let Some((k, v)) = parse_env_line(line) {
                        if *show {
                            println!("  {k}={v}");
                        } else {
                            println!("  {k}=***");
                        }
                        count += 1;
                    }
                }

                if count == 0 {
                    println!("  (no secrets set)");
                } else if !show {
                    println!();
                    println!("Tip: use --show to reveal values");
                }

                Ok(())
            }
            SecretsListMode::Remote(options) => {
                options.validate()?;
                let operation = "remote secrets list";
                let ctx = CliRuntime::new("remote secrets list")?;
                let secrets = match options.target.scope {
                    RemoteSecretScope::Service => {
                        let project_ref = options.target.project_ref().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --project".to_string(),
                            )
                        })?;
                        let project = resolve_project(&ctx, project_ref, operation)?;
                        let app_code = options.target.service_code().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --service".to_string(),
                            )
                        })?;
                        let app_code = resolve_remote_service_code(&project, app_code)?;
                        ctx.block_on(ctx.client.list_service_secrets(project.id, &app_code))
                            .map_err(|error| remap_remote_secret_error(operation, error))?
                    }
                    RemoteSecretScope::Server => {
                        let server_id = options.target.server_id().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Server-scoped secrets require --server-id".to_string(),
                            )
                        })?;
                        ctx.block_on(ctx.client.list_server_secrets(server_id))
                            .map_err(|error| remap_remote_secret_error(operation, error))?
                    }
                };

                print_remote_secret_list(&secrets, options.json)?;
                Ok(())
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets apps
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub struct SecretsAppsCommand {
    project: String,
    json: bool,
}

impl SecretsAppsCommand {
    pub fn new(project: String, json: bool) -> Self {
        Self { project, json }
    }
}

impl CallableTrait for SecretsAppsCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let operation = "remote project apps list";
        let ctx = CliRuntime::new(operation)?;
        let project = resolve_project(&ctx, &self.project, operation)?;
        let apps = ctx
            .block_on(ctx.client.list_project_apps(project.id))
            .map_err(|error| remap_remote_secret_error(operation, error))?;

        print_project_app_list(&apps, self.json)?;
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets delete
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets delete KEY [--file .env]`
pub struct SecretsDeleteCommand {
    mode: SecretsDeleteMode,
}

#[derive(Debug)]
enum SecretsDeleteMode {
    Local {
        key: String,
        file: Option<String>,
    },
    Remote {
        key: String,
        target: RemoteSecretTarget,
    },
}

impl SecretsDeleteCommand {
    pub fn new(key: String, file: Option<String>) -> Self {
        Self {
            mode: SecretsDeleteMode::Local { key, file },
        }
    }

    pub fn new_remote(
        key: String,
        scope: RemoteSecretScope,
        project: Option<String>,
        service: Option<String>,
        server_id: Option<i32>,
    ) -> Self {
        Self {
            mode: SecretsDeleteMode::Remote {
                key,
                target: RemoteSecretTarget::new(scope, project, service, server_id),
            },
        }
    }
}

impl CallableTrait for SecretsDeleteCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        match &self.mode {
            SecretsDeleteMode::Local { key, file } => {
                let env_path = resolve_env_path(file.as_deref())?;

                if !env_path.exists() {
                    return Err(Box::new(CliError::EnvFileNotFound { path: env_path }));
                }

                let lines = read_env_lines(&env_path)?;
                let before_len = lines.len();
                let filtered: Vec<String> = lines
                    .into_iter()
                    .filter(|line| {
                        if let Some((k, _)) = parse_env_line(line) {
                            k != *key
                        } else {
                            true
                        }
                    })
                    .collect();

                if filtered.len() == before_len {
                    return Err(Box::new(CliError::SecretKeyNotFound { key: key.clone() }));
                }

                write_env_lines(&env_path, &filtered)?;
                println!("✓ Deleted {} from {}", key, env_path.display());
                Ok(())
            }
            SecretsDeleteMode::Remote { key, target } => {
                validate_secret_name(key)?;
                target.validate()?;
                let operation = "remote secrets delete";
                let ctx = CliRuntime::new("remote secrets delete")?;

                match target.scope {
                    RemoteSecretScope::Service => {
                        let project_ref = target.project_ref().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --project".to_string(),
                            )
                        })?;
                        let project = resolve_project(&ctx, project_ref, operation)?;
                        let app_code = target.service_code().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Service-scoped secrets require --service".to_string(),
                            )
                        })?;
                        let app_code = resolve_remote_service_code(&project, app_code)?;
                        ctx.block_on(ctx.client.delete_service_secret(project.id, &app_code, key))
                            .map_err(|error| remap_remote_secret_error(operation, error))?;
                        println!("✓ Deleted service secret {} from {}", key, app_code);
                    }
                    RemoteSecretScope::Server => {
                        let server_id = target.server_id().ok_or_else(|| {
                            CliError::ConfigValidation(
                                "Server-scoped secrets require --server-id".to_string(),
                            )
                        })?;
                        ctx.block_on(ctx.client.delete_server_secret(server_id, key))
                            .map_err(|error| remap_remote_secret_error(operation, error))?;
                        println!("✓ Deleted server secret {} from server {}", key, server_id);
                    }
                }

                Ok(())
            }
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets validate
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets validate [--file stacker.yml]`
///
/// Scans `stacker.yml` for `${VAR}` references and checks that every
/// referenced variable is present in the `.env` file or the current
/// environment.
pub struct SecretsValidateCommand {
    pub file: Option<String>,
}

impl SecretsValidateCommand {
    pub fn new(file: Option<String>) -> Self {
        Self { file }
    }
}

impl CallableTrait for SecretsValidateCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.file.as_deref().unwrap_or(DEFAULT_CONFIG_FILE);
        let path = Path::new(config_path);

        if !path.exists() {
            return Err(Box::new(CliError::ConfigNotFound {
                path: path.to_path_buf(),
            }));
        }

        let raw = std::fs::read_to_string(path)?;

        // Collect all ${VAR} references
        let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
        let refs: Vec<String> = re
            .captures_iter(&raw)
            .map(|cap| cap[1].to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if refs.is_empty() {
            println!("✓ No environment variable references found in {config_path}");
            return Ok(());
        }

        // Load .env file values
        let env_path = resolve_env_path(None)?;
        let env_lines = read_env_lines(&env_path).unwrap_or_default();
        let mut env_map = std::collections::HashMap::new();
        for line in &env_lines {
            if let Some((k, v)) = parse_env_line(line) {
                env_map.insert(k, v);
            }
        }

        let mut missing: Vec<String> = Vec::new();
        let mut found: Vec<String> = Vec::new();

        for var in &refs {
            if env_map.contains_key(var.as_str()) || std::env::var(var).is_ok() {
                found.push(var.clone());
            } else {
                missing.push(var.clone());
            }
        }

        // Sort for deterministic output
        found.sort();
        missing.sort();

        for var in &found {
            println!("  ✓ {var}");
        }
        for var in &missing {
            eprintln!("  ✗ {var}  (not set)");
        }

        if missing.is_empty() {
            println!();
            println!("✓ All {} variable(s) are set", refs.len());
            Ok(())
        } else {
            Err(Box::new(CliError::ConfigValidation(format!(
                "{} variable(s) referenced in {config_path} are not set: {}",
                missing.len(),
                missing.join(", ")
            ))))
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Security tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── SECURITY: Path traversal via --file flag ──────
    // CWE-22: Improper Limitation of a Pathname to a Restricted Directory
    //
    // The --file flag accepts arbitrary paths. An attacker could read or
    // write files outside the project directory using `../../etc/crontab`
    // style paths. The resolve_env_path function does not sanitize paths.

    #[test]
    fn test_resolve_env_path_rejects_path_traversal() {
        let result = resolve_env_path(Some("../../etc/passwd"));
        assert!(result.is_err(), "Path traversal must be rejected");
    }

    #[test]
    fn test_resolve_env_path_allows_absolute_path_without_traversal() {
        let result = resolve_env_path(Some("/etc/passwd"));
        assert!(
            result.is_ok(),
            "Absolute paths without traversal are allowed"
        );
    }

    #[test]
    fn test_resolve_env_path_accepts_relative_safe_path() {
        let result = resolve_env_path(Some("config/.env"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("config/.env"));
    }

    // ── SECURITY: Env file has no restricted permissions ──
    // CWE-732: Incorrect Permission Assignment for Critical Resource
    //
    // The .env file may contain secrets but write_env_lines does not
    // set restrictive file permissions (unlike credentials.json which
    // correctly sets 0o600).

    #[test]
    #[cfg(unix)]
    fn test_env_file_permissions_are_restricted() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        let lines = vec!["DB_PASSWORD=supersecret".to_string()];
        write_env_lines(&env_path, &lines).unwrap();

        let perms = std::fs::metadata(&env_path).unwrap().permissions();
        let mode = perms.mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "Env file containing secrets must have 0o600 permissions"
        );
    }

    // ── SECURITY: Key validation ──────────────────────
    // CWE-20: Improper Input Validation
    //
    // Secret keys can contain newlines or equals signs that break .env parsing.

    #[test]
    fn test_secrets_set_rejects_empty_key() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        let cmd = SecretsSetCommand::new(
            "=value".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        let result = cmd.call();
        assert!(result.is_err(), "Expected error for empty key");
    }

    #[test]
    fn test_secrets_set_key_with_newline_is_rejected() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        let cmd = SecretsSetCommand::new(
            "LEGIT\nMALICIOUS_KEY=injected".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        let result = cmd.call();
        assert!(result.is_err(), "Keys with newlines must be rejected");
    }

    #[test]
    fn test_secrets_set_key_with_spaces_is_rejected() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        let cmd = SecretsSetCommand::new(
            "BAD KEY=value".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        let result = cmd.call();
        assert!(result.is_err(), "Keys with spaces must be rejected");
    }

    #[test]
    fn test_secrets_set_valid_key_accepted() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        let cmd = SecretsSetCommand::new(
            "_MY_VAR_123=value".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        let result = cmd.call();
        assert!(result.is_ok(), "Valid env key must be accepted");
    }

    // ── SECURITY: Value parsing edge cases ────────────
    // CWE-20: Improper Input Validation

    #[test]
    fn test_parse_env_line_basic() {
        let (k, v) = parse_env_line("FOO=bar").unwrap();
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar");
    }

    #[test]
    fn test_parse_env_line_quoted() {
        let (k, v) = parse_env_line("FOO=\"bar baz\"").unwrap();
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar baz");
    }

    #[test]
    fn test_parse_env_line_single_quoted() {
        let (k, v) = parse_env_line("FOO='bar baz'").unwrap();
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar baz");
    }

    #[test]
    fn test_parse_env_line_comment() {
        assert!(parse_env_line("# this is a comment").is_none());
    }

    #[test]
    fn test_parse_env_line_empty() {
        assert!(parse_env_line("").is_none());
    }

    #[test]
    fn test_parse_env_line_no_equals() {
        assert!(parse_env_line("JUST_A_KEY").is_none());
    }

    #[test]
    fn test_parse_env_line_value_with_equals() {
        let (k, v) = parse_env_line("FOO=bar=baz").unwrap();
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar=baz");
    }

    // ── Round-trip tests ──────────────────────────────

    #[test]
    fn test_write_and_read_env_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");
        let lines = vec!["FOO=bar".to_string(), "BAZ=qux".to_string()];
        write_env_lines(&path, &lines).unwrap();
        let read = read_env_lines(&path).unwrap();
        assert_eq!(read, vec!["FOO=bar", "BAZ=qux"]);
    }

    #[test]
    fn test_read_env_lines_nonexistent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist");
        let lines = read_env_lines(&path).unwrap();
        assert!(lines.is_empty());
    }

    // ── Functional secrets tests ──────────────────────

    #[test]
    fn test_secrets_set_and_get() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");

        let set_cmd = SecretsSetCommand::new(
            "MY_SECRET=hello123".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        set_cmd.call().unwrap();

        // Verify the file was written
        let content = std::fs::read_to_string(&env_path).unwrap();
        assert!(content.contains("MY_SECRET=hello123"));
    }

    #[test]
    fn test_secrets_set_updates_existing_key() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "MY_KEY=old_value\nOTHER=keep\n").unwrap();

        let cmd = SecretsSetCommand::new(
            "MY_KEY=new_value".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        cmd.call().unwrap();

        let content = std::fs::read_to_string(&env_path).unwrap();
        assert!(content.contains("MY_KEY=new_value"));
        assert!(!content.contains("old_value"));
        assert!(content.contains("OTHER=keep"));
    }

    #[test]
    fn test_secrets_delete_removes_key() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "KEEP=yes\nDELETE_ME=gone\n").unwrap();

        let cmd = SecretsDeleteCommand::new(
            "DELETE_ME".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        cmd.call().unwrap();

        let content = std::fs::read_to_string(&env_path).unwrap();
        assert!(!content.contains("DELETE_ME"));
        assert!(content.contains("KEEP=yes"));
    }

    #[test]
    fn test_secrets_delete_nonexistent_key_errors() {
        let dir = TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "FOO=bar\n").unwrap();

        let cmd = SecretsDeleteCommand::new(
            "NONEXISTENT".to_string(),
            Some(env_path.to_string_lossy().to_string()),
        );
        let result = cmd.call();
        assert!(result.is_err());
    }

    #[test]
    fn test_remote_service_target_requires_project() {
        let target = RemoteSecretTarget::new(
            RemoteSecretScope::Service,
            None,
            Some("web".to_string()),
            None,
        );

        let error = target.validate().unwrap_err().to_string();
        assert!(error.contains("--project"));
    }

    #[test]
    fn test_remote_service_target_requires_service() {
        let target = RemoteSecretTarget::new(
            RemoteSecretScope::Service,
            Some("project-1".to_string()),
            None,
            None,
        );

        let error = target.validate().unwrap_err().to_string();
        assert!(error.contains("--service"));
    }

    #[test]
    fn test_remote_server_target_rejects_project_and_service() {
        let target = RemoteSecretTarget::new(
            RemoteSecretScope::Server,
            Some("project-1".to_string()),
            Some("web".to_string()),
            Some(42),
        );

        let error = target.validate().unwrap_err().to_string();
        assert!(error.contains("--project or --service"));
    }

    #[test]
    fn test_remote_server_target_requires_server_id() {
        let target = RemoteSecretTarget::new(RemoteSecretScope::Server, None, None, None);

        let error = target.validate().unwrap_err().to_string();
        assert!(error.contains("--server-id"));
    }

    #[test]
    fn test_remote_set_debug_redacts_inline_secret_value() {
        let options = RemoteSecretWriteOptions {
            name: "NPM_TOKEN".to_string(),
            target: RemoteSecretTarget::new(RemoteSecretScope::Server, None, None, Some(42)),
            body: Some("supersecret".to_string()),
            body_file: None,
        };

        let debug_output = format!("{options:?}");
        assert!(!debug_output.contains("supersecret"));
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn test_remote_set_resolves_inline_secret_value() {
        let options = RemoteSecretWriteOptions {
            name: "NPM_TOKEN".to_string(),
            target: RemoteSecretTarget::new(RemoteSecretScope::Server, None, None, Some(42)),
            body: Some("supersecret".to_string()),
            body_file: None,
        };

        assert_eq!(
            resolve_remote_secret_value(&options).unwrap(),
            "supersecret"
        );
    }

    #[test]
    fn test_remote_set_validates_scope_before_runtime_execution() {
        let options = RemoteSecretWriteOptions {
            name: "NPM_TOKEN".to_string(),
            target: RemoteSecretTarget::new(
                RemoteSecretScope::Server,
                Some("project-1".to_string()),
                None,
                Some(42),
            ),
            body: Some("supersecret".to_string()),
            body_file: None,
        };

        let error = options.validate().unwrap_err().to_string();
        assert!(error.contains("--project or --service"));
    }

    #[test]
    fn test_remote_command_constructor_keeps_scope_metadata() {
        let command = SecretsSetCommand::new_remote(
            "NPM_TOKEN".to_string(),
            RemoteSecretScope::Server,
            None,
            None,
            Some(42),
            Some("supersecret".to_string()),
            None,
        );

        match command.mode {
            SecretsSetMode::Remote(options) => {
                assert_eq!(options.target.scope, RemoteSecretScope::Server);
                assert_eq!(options.target.server_id, Some(42));
            }
            SecretsSetMode::Local { .. } => panic!("expected remote command mode"),
        }
    }

    #[test]
    fn test_remote_secret_error_remaps_deploy_failed_context() {
        let error = remap_remote_secret_error(
            "remote secrets list",
            CliError::DeployFailed {
                target: crate::cli::config_parser::DeployTarget::Cloud,
                reason: "Stacker server GET /project/1/apps/web/secrets failed (403):".to_string(),
            },
        );

        let rendered = error.to_string();
        assert!(rendered.contains("remote secrets list failed"));
        assert!(!rendered.contains("Deployment to cloud failed"));
    }

    #[test]
    fn test_project_app_codes_extracts_declared_web_codes() {
        let project = ProjectInfo {
            id: 7,
            name: "syncopia".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({
                "custom": {
                    "web": [
                        {"code": "app"},
                        {"code": "device-apis"},
                        {"code": "upload"}
                    ]
                }
            }),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            project_app_codes(&project),
            vec![
                "app".to_string(),
                "device-apis".to_string(),
                "upload".to_string()
            ]
        );
    }

    #[test]
    fn test_resolve_remote_service_code_matches_case_insensitively() {
        let project = ProjectInfo {
            id: 7,
            name: "syncopia".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({
                "custom": {
                    "web": [
                        {"code": "device-apis"}
                    ]
                }
            }),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            resolve_remote_service_code(&project, "Device-APIs").unwrap(),
            "device-apis"
        );
    }

    #[test]
    fn test_resolve_remote_service_code_reports_available_codes() {
        let project = ProjectInfo {
            id: 7,
            name: "syncopia".to_string(),
            user_id: "user-1".to_string(),
            metadata: serde_json::json!({
                "custom": {
                    "web": [
                        {"code": "app"},
                        {"code": "device-apis"}
                    ]
                }
            }),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let error = resolve_remote_service_code(&project, "device-api")
            .unwrap_err()
            .to_string();
        assert!(error.contains("Available app codes: app, device-apis"));
    }

    #[test]
    fn test_print_project_app_list_renders_empty_state() {
        print_project_app_list(&[], false).unwrap();
    }
}
