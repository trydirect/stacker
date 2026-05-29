use std::io::{self, IsTerminal};

use crate::cli::credentials::{browser_login, login, CredentialsManager, HttpOAuthClient, LoginRequest};
use crate::cli::user_config::UserConfig;
use crate::console::commands::CallableTrait;
use dialoguer::{Password, Select};

pub struct LoginCommand {
    pub org: Option<String>,
    pub domain: Option<String>,
    pub auth_url: Option<String>,
    pub server_url: Option<String>,
    pub browser: bool,
    /// Explicit --provider value; None means "ask interactively".
    pub provider: Option<String>,
    /// Pre-filled email for username/password flow; also disables browser flow.
    pub user: Option<String>,
}

impl LoginCommand {
    pub fn new(
        org: Option<String>,
        domain: Option<String>,
        auth_url: Option<String>,
        server_url: Option<String>,
        browser: bool,
        provider: Option<String>,
        user: Option<String>,
    ) -> Self {
        let cfg = UserConfig::load();
        Self {
            org,
            domain,
            auth_url,
            server_url,
            browser: browser || cfg.browser_default(),
            provider,
            user,
        }
    }

    /// Returns the OAuth provider code to use, or `None` if the user chose
    /// username/password from the interactive menu.
    ///
    /// Resolution order:
    ///   1. `--provider <code>` flag → use it directly, no prompt.
    ///   2. Interactive tty → show a Select menu.
    ///   3. Non-interactive → fall back to the config/default value.
    fn prompt_provider(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(ref p) = self.provider {
            return Ok(Some(p.clone()));
        }

        if io::stdin().is_terminal() {
            const PROVIDERS: &[(&str, Option<&str>)] = &[
                ("Google", Some("gc")),
                ("GitHub", Some("gh")),
                ("Microsoft", Some("azu")),
                ("Username / Password", None),
            ];
            let labels: Vec<&str> = PROVIDERS.iter().map(|(l, _)| *l).collect();
            let idx = Select::new()
                .with_prompt("Select auth provider")
                .items(&labels)
                .default(0)
                .interact()
                .map_err(|e| format!("Prompt error: {e}"))?;
            return Ok(PROVIDERS[idx].1.map(str::to_string));
        }

        Ok(Some(UserConfig::load().provider_default()))
    }

    fn read_line(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        eprint!("{}", prompt);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }

    fn read_password(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        if io::stdin().is_terminal() {
            let cleaned = prompt.trim().trim_end_matches(':').trim();
            let input = Password::new()
                .with_prompt(cleaned)
                .interact()
                .map_err(|e| format!("Failed to read password: {}", e))?;
            Ok(input.trim().to_string())
        } else {
            Self::read_line(prompt)
        }
    }

    fn resolve_auth_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        let cfg = UserConfig::load();
        self.auth_url
            .clone()
            .or_else(|| std::env::var("STACKER_AUTH_URL").ok())
            .or_else(|| std::env::var("STACKER_API_URL").ok())
            .or_else(|| cfg.auth_url)
            .ok_or_else(|| {
                "Missing auth URL. Pass --auth-url <user-service-url> or set STACKER_AUTH_URL.".into()
            })
    }

    fn resolve_server_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        let cfg = UserConfig::load();
        self.server_url
            .clone()
            .or_else(|| std::env::var("STACKER_URL").ok())
            .or_else(|| cfg.server_url)
            .ok_or_else(|| {
                "Missing Stacker API URL. Pass --server-url <stacker-api-url> or set STACKER_URL.".into()
            })
    }
}

impl CallableTrait for LoginCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        // --user/-u always means email/password; skip prompt and browser.
        let use_browser = self.user.is_none() && (self.browser || io::stdin().is_terminal());

        if use_browser {
            // Resolve provider — may show an interactive menu.
            let provider = self.prompt_provider()?;

            if let Some(provider) = provider {
                let auth_url = self.resolve_auth_url()?;
                let server_url = self.resolve_server_url()?;
                let manager = CredentialsManager::with_default_store();

                let creds = browser_login(
                    &manager,
                    &auth_url,
                    &server_url,
                    &provider,
                    self.org.as_deref(),
                    self.domain.as_deref(),
                )?;

                eprintln!("✓ {}", creds);
                if let Some(org) = &creds.org {
                    eprintln!("  Organization: {}", org);
                }
                if let Some(domain) = &creds.domain {
                    eprintln!("  Domain: {}", domain);
                }
                if let Some(server_url) = &creds.server_url {
                    eprintln!("  Stacker API: {}", server_url);
                }
                return Ok(());
            }
            // provider == None → user chose "Username / Password" from the menu;
            // fall through to the email/password path below.
        }

        // Email/password flow — email may be pre-filled via --user/-u.
        let email = match self.user.as_deref() {
            Some(e) if !e.is_empty() => e.to_string(),
            _ => {
                let e = Self::read_line("Email: ")?;
                if e.is_empty() {
                    return Err("Email cannot be empty".into());
                }
                e
            }
        };

        let password = Self::read_password("Password: ")?;
        if password.is_empty() {
            return Err("Password cannot be empty".into());
        }

        let request = LoginRequest {
            email,
            password,
            auth_url: self.auth_url.clone(),
            server_url: self.server_url.clone(),
            org: self.org.clone(),
            domain: self.domain.clone(),
        };

        let manager = CredentialsManager::with_default_store();
        let oauth = HttpOAuthClient;

        let creds = login(&manager, &oauth, &request)?;

        eprintln!("✓ {}", creds);
        if let Some(org) = &creds.org {
            eprintln!("  Organization: {}", org);
        }
        if let Some(domain) = &creds.domain {
            eprintln!("  Domain: {}", domain);
        }
        if let Some(server_url) = &creds.server_url {
            eprintln!("  Stacker API: {}", server_url);
        }

        Ok(())
    }
}
