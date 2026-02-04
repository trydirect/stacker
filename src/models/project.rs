use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use uuid::Uuid;

/// Regex for valid Unix directory names (cached on first use)
fn valid_dir_name_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        // Must start with alphanumeric or underscore
        // Can contain alphanumeric, underscore, hyphen, dot
        // Length 1-255 characters
        Regex::new(r"^[a-zA-Z0-9_][a-zA-Z0-9_\-.]{0,254}$").unwrap()
    })
}

/// Error type for project name validation
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectNameError {
    Empty,
    TooLong(usize),
    InvalidCharacters(String),
    ReservedName(String),
}

impl std::fmt::Display for ProjectNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectNameError::Empty => write!(f, "Project name cannot be empty"),
            ProjectNameError::TooLong(len) => {
                write!(f, "Project name too long ({} chars, max 255)", len)
            }
            ProjectNameError::InvalidCharacters(name) => {
                write!(
                    f,
                    "Project name '{}' contains invalid characters. Use only alphanumeric, underscore, hyphen, or dot",
                    name
                )
            }
            ProjectNameError::ReservedName(name) => {
                write!(f, "Project name '{}' is reserved", name)
            }
        }
    }
}

impl std::error::Error for ProjectNameError {}

/// Reserved directory names that should not be used as project names
const RESERVED_NAMES: &[&str] = &[
    ".",
    "..",
    "root",
    "home",
    "etc",
    "var",
    "tmp",
    "usr",
    "bin",
    "sbin",
    "lib",
    "lib64",
    "opt",
    "proc",
    "sys",
    "dev",
    "boot",
    "mnt",
    "media",
    "srv",
    "run",
    "lost+found",
    "trydirect",
];

/// Validate a project name for use as a Unix directory name
pub fn validate_project_name(name: &str) -> Result<(), ProjectNameError> {
    // Check empty
    if name.is_empty() {
        return Err(ProjectNameError::Empty);
    }

    // Check length
    if name.len() > 255 {
        return Err(ProjectNameError::TooLong(name.len()));
    }

    // Check reserved names (case-insensitive)
    let lower = name.to_lowercase();
    if RESERVED_NAMES.contains(&lower.as_str()) {
        return Err(ProjectNameError::ReservedName(name.to_string()));
    }

    // Check valid characters
    if !valid_dir_name_regex().is_match(name) {
        return Err(ProjectNameError::InvalidCharacters(name.to_string()));
    }

    Ok(())
}

/// Sanitize a project name to be a valid Unix directory name
/// Replaces invalid characters and ensures the result is valid
pub fn sanitize_project_name(name: &str) -> String {
    if name.is_empty() {
        return "project".to_string();
    }

    // Convert to lowercase and replace invalid chars with underscore
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                // First char must be alphanumeric or underscore
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            } else {
                // Subsequent chars can also include hyphen and dot
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                    c
                } else {
                    '_'
                }
            }
        })
        .collect();

    // Truncate if too long
    let truncated: String = sanitized.chars().take(255).collect();

    // Check if it's a reserved name
    if RESERVED_NAMES.contains(&truncated.as_str()) {
        return format!("project_{}", truncated);
    }

    if truncated.is_empty() {
        "project".to_string()
    } else {
        truncated
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: i32,         // id - is a unique identifier for the app project
    pub stack_id: Uuid,  // external project ID
    pub user_id: String, // external unique identifier for the user
    pub name: String,
    // pub metadata: sqlx::types::Json<String>,
    pub metadata: Value, //json type
    pub request_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_template_id: Option<Uuid>, // marketplace template UUID
    pub template_version: Option<String>, // marketplace template version
}

impl Project {
    pub fn new(user_id: String, name: String, metadata: Value, request_json: Value) -> Self {
        Self {
            id: 0,
            stack_id: Uuid::new_v4(),
            user_id,
            name,
            metadata,
            request_json,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source_template_id: None,
            template_version: None,
        }
    }

    /// Validate the project name for use as a directory
    pub fn validate_name(&self) -> Result<(), ProjectNameError> {
        validate_project_name(&self.name)
    }

    /// Get the sanitized directory name for this project (lowercase, safe for Unix)
    pub fn safe_dir_name(&self) -> String {
        sanitize_project_name(&self.name)
    }

    /// Get the full deploy directory path for this project
    /// Uses the provided base_dir, or DEFAULT_DEPLOY_DIR env var, or defaults to /home/trydirect
    pub fn deploy_dir(&self, base_dir: Option<&str>) -> String {
        let default_base =
            std::env::var("DEFAULT_DEPLOY_DIR").unwrap_or_else(|_| "/home/trydirect".to_string());
        let base = base_dir.unwrap_or(&default_base);
        format!("{}/{}", base.trim_end_matches('/'), self.safe_dir_name())
    }

    /// Get the deploy directory using deployment_hash (for backwards compatibility)
    pub fn deploy_dir_with_hash(&self, base_dir: Option<&str>, deployment_hash: &str) -> String {
        let default_base =
            std::env::var("DEFAULT_DEPLOY_DIR").unwrap_or_else(|_| "/home/trydirect".to_string());
        let base = base_dir.unwrap_or(&default_base);
        format!("{}/{}", base.trim_end_matches('/'), deployment_hash)
    }
}

impl Default for Project {
    fn default() -> Self {
        Project {
            id: 0,
            stack_id: Default::default(),
            user_id: "".to_string(),
            name: "".to_string(),
            metadata: Default::default(),
            request_json: Default::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
            source_template_id: None,
            template_version: None,
        }
    }
}
