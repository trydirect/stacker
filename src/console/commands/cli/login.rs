use crate::console::commands::CallableTrait;
use crate::cli::credentials::{
    CredentialsManager, FileCredentialStore, HttpOAuthClient, LoginRequest, login,
};

/// `stacker login [--org <name>] [--domain <domain>] [--auth-url <url>]`
///
/// Authenticates with the TryDirect platform via OAuth2 and stores
/// credentials in `~/.config/stacker/credentials.json`.
///
/// Prompts for email/password on stdin when running interactively.
pub struct LoginCommand {
    pub org: Option<String>,
    pub domain: Option<String>,
    pub auth_url: Option<String>,
}

impl LoginCommand {
    pub fn new(org: Option<String>, domain: Option<String>, auth_url: Option<String>) -> Self {
        Self {
            org,
            domain,
            auth_url,
        }
    }

    /// Read a line from stdin (used for email/password prompts).
    fn read_line(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
        eprint!("{}", prompt);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        Ok(input.trim().to_string())
    }
}

impl CallableTrait for LoginCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let email = Self::read_line("Email: ")?;
        if email.is_empty() {
            return Err("Email cannot be empty".into());
        }

        let password = Self::read_line("Password: ")?;
        if password.is_empty() {
            return Err("Password cannot be empty".into());
        }

        let request = LoginRequest {
            email,
            password,
            auth_url: self.auth_url.clone(),
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

        Ok(())
    }
}
