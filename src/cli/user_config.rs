use std::path::PathBuf;

use serde::Deserialize;

/// User-level configuration stored at `~/.config/stacker/config.yml`.
///
/// Written once by `install.sh` with platform defaults; the user can edit it
/// manually. Values here are overridden by CLI flags and environment variables.
///
/// Priority (highest to lowest):
///   CLI flag → environment variable → `~/.config/stacker/config.yml` → built-in default
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserConfig {
    /// TryDirect user-service base URL (e.g. `https://try.direct/server/user`).
    pub auth_url: Option<String>,
    /// Stacker API URL (e.g. `https://try.direct/stacker`).
    pub server_url: Option<String>,
    /// Default organisation slug for multi-org accounts.
    pub org: Option<String>,
    /// Login-specific preferences.
    pub login: Option<LoginConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LoginConfig {
    /// Use browser-based OAuth2 flow by default (`true`) or email/password (`false`).
    pub browser: Option<bool>,
    /// Default OAuth provider code: `gc` (Google), `gh` (GitHub), etc.
    pub provider: Option<String>,
}

impl UserConfig {
    /// Load from `~/.config/stacker/config.yml`.
    /// Returns `Default` silently if the file is absent or unreadable.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        serde_yaml::from_str(&content).unwrap_or_default()
    }

    /// Platform-appropriate path for the user config file.
    pub fn config_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|_| PathBuf::from("."));
        base.join("stacker").join("config.yml")
    }

    /// Default browser preference: `true` unless explicitly disabled.
    pub fn browser_default(&self) -> bool {
        self.login.as_ref().and_then(|l| l.browser).unwrap_or(true)
    }

    /// Default provider: `gc` (Google) unless overridden.
    pub fn provider_default(&self) -> String {
        self.login
            .as_ref()
            .and_then(|l| l.provider.clone())
            .unwrap_or_else(|| "gc".to_string())
    }
}
