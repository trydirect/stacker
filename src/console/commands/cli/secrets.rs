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

use std::path::{Path, PathBuf};

use crate::cli::error::CliError;
use crate::console::commands::CallableTrait;

const DEFAULT_ENV_FILE: &str = ".env";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets set
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets set KEY=VALUE [--file .env]`
pub struct SecretsSetCommand {
    pub key_value: String,
    pub file: Option<String>,
}

impl SecretsSetCommand {
    pub fn new(key_value: String, file: Option<String>) -> Self {
        Self { key_value, file }
    }
}

impl CallableTrait for SecretsSetCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Parse "KEY=VALUE"
        let pos = self.key_value.find('=').ok_or_else(|| {
            CliError::ConfigValidation(
                "Expected KEY=VALUE format (e.g. DB_PASS=secret)".to_string(),
            )
        })?;
        let key = self.key_value[..pos].trim().to_string();
        let value = self.key_value[pos + 1..].to_string();

        let valid_key = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
        if !valid_key.is_match(&key) {
            return Err(Box::new(CliError::ConfigValidation(format!(
                "Invalid key '{}': must match [A-Za-z_][A-Za-z0-9_]*",
                key
            ))));
        }

        let env_path = resolve_env_path(self.file.as_deref())?;
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
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets get
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets get KEY [--file .env] [--show]`
pub struct SecretsGetCommand {
    pub key: String,
    pub file: Option<String>,
    pub show: bool,
}

impl SecretsGetCommand {
    pub fn new(key: String, file: Option<String>, show: bool) -> Self {
        Self { key, file, show }
    }
}

impl CallableTrait for SecretsGetCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let env_path = resolve_env_path(self.file.as_deref())?;

        if !env_path.exists() {
            return Err(Box::new(CliError::EnvFileNotFound { path: env_path }));
        }

        let lines = read_env_lines(&env_path)?;
        for line in &lines {
            if let Some((k, v)) = parse_env_line(line) {
                if k == self.key {
                    if self.show {
                        println!("{k}={v}");
                    } else {
                        println!("{k}=***");
                    }
                    return Ok(());
                }
            }
        }

        Err(Box::new(CliError::SecretKeyNotFound {
            key: self.key.clone(),
        }))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets list
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets list [--file .env] [--show]`
pub struct SecretsListCommand {
    pub file: Option<String>,
    pub show: bool,
}

impl SecretsListCommand {
    pub fn new(file: Option<String>, show: bool) -> Self {
        Self { file, show }
    }
}

impl CallableTrait for SecretsListCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let env_path = resolve_env_path(self.file.as_deref())?;

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
                if self.show {
                    println!("  {k}={v}");
                } else {
                    println!("  {k}=***");
                }
                count += 1;
            }
        }

        if count == 0 {
            println!("  (no secrets set)");
        } else if !self.show {
            println!();
            println!("Tip: use --show to reveal values");
        }

        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// secrets delete
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `stacker secrets delete KEY [--file .env]`
pub struct SecretsDeleteCommand {
    pub key: String,
    pub file: Option<String>,
}

impl SecretsDeleteCommand {
    pub fn new(key: String, file: Option<String>) -> Self {
        Self { key, file }
    }
}

impl CallableTrait for SecretsDeleteCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let env_path = resolve_env_path(self.file.as_deref())?;

        if !env_path.exists() {
            return Err(Box::new(CliError::EnvFileNotFound { path: env_path }));
        }

        let lines = read_env_lines(&env_path)?;
        let before_len = lines.len();
        let filtered: Vec<String> = lines
            .into_iter()
            .filter(|line| {
                if let Some((k, _)) = parse_env_line(line) {
                    k != self.key
                } else {
                    true // preserve comments / blank lines
                }
            })
            .collect();

        if filtered.len() == before_len {
            return Err(Box::new(CliError::SecretKeyNotFound {
                key: self.key.clone(),
            }));
        }

        write_env_lines(&env_path, &filtered)?;
        println!("✓ Deleted {} from {}", self.key, env_path.display());
        Ok(())
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

    // ── Helper ────────────────────────────────────────
    fn write_env(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

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
}
