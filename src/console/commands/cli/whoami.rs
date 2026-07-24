use std::path::Path;

use crate::cli::credentials::{CredentialsManager, StoredCredentials};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::user_config::UserConfig;
use crate::connectors::user_service::plan::{
    get_subscription_plan_blocking, subscription_plan_lines, user_service_url_from_sources,
};
use crate::console::commands::CallableTrait;

pub struct WhoamiCommand;

impl WhoamiCommand {
    pub fn new() -> Self {
        Self
    }
}

fn describe_saved_login(creds: Option<&StoredCredentials>) -> Vec<String> {
    match creds {
        Some(creds) => {
            let mut lines = vec![format!("{}", creds)];
            if let Some(server_url) = &creds.server_url {
                lines.push(format!("  Stacker API:   {}", server_url));
            }
            if let Some(org) = &creds.org {
                lines.push(format!("  Organization:  {}", org));
            }
            if let Some(domain) = &creds.domain {
                lines.push(format!("  Domain:        {}", domain));
            }
            lines.push(format!(
                "  Expires at:    {}",
                creds.expires_at.to_rfc3339()
            ));
            lines
        }
        None => vec![
            "Not logged in".to_string(),
            "  Run: stacker login".to_string(),
        ],
    }
}

fn load_project_lock(
    project_dir: &Path,
) -> Result<Option<DeploymentLock>, crate::cli::error::CliError> {
    DeploymentLock::load_active(project_dir)
}

fn describe_project_lock(lock: Option<&DeploymentLock>) -> Vec<String> {
    match lock {
        Some(lock) => {
            let mut lines = vec!["Current project:".to_string()];
            lines.push(format!("  Target:        {}", lock.target));
            if let Some(project_name) = &lock.project_name {
                lines.push(format!("  Project name:  {}", project_name));
            }
            match &lock.stacker_email {
                Some(email) => lines.push(format!("  Deployed by:   {}", email)),
                None => lines
                    .push("  Deployed by:   unknown (lock predates account tracking)".to_string()),
            }
            if let Some(server_name) = &lock.server_name {
                lines.push(format!("  Server name:   {}", server_name));
            }
            if let Some(ssh_user) = &lock.ssh_user {
                lines.push(format!("  SSH user:      {}", ssh_user));
            }
            lines.push(format!("  Recorded at:   {}", lock.deployed_at));
            lines
        }
        None => vec!["Current project: no deployment context found".to_string()],
    }
}

fn resolve_user_service_url(creds: &StoredCredentials) -> Option<String> {
    let cfg = UserConfig::load();
    user_service_url_from_sources(
        std::env::var("STACKER_AUTH_URL").ok(),
        std::env::var("STACKER_API_URL").ok(),
        creds.server_url.clone(),
        cfg.auth_url,
    )
}

fn describe_subscription_unavailable(reason: &str) -> Vec<String> {
    vec![
        "Subscription: unavailable".to_string(),
        format!("  Reason:        {}", reason),
    ]
}

fn load_subscription_plan(creds: Option<&StoredCredentials>) -> Vec<String> {
    let Some(creds) = creds else {
        return describe_subscription_unavailable("not logged in");
    };

    if creds.is_expired() {
        return describe_subscription_unavailable("login expired; run `stacker login`");
    }

    let Some(user_service_url) = resolve_user_service_url(creds) else {
        return describe_subscription_unavailable(
            "missing User Service URL; set STACKER_AUTH_URL or configure auth_url",
        );
    };

    match get_subscription_plan_blocking(&user_service_url, &creds.access_token) {
        Ok(plan) => subscription_plan_lines(&plan),
        Err(error) => describe_subscription_unavailable(&format!("failed to fetch plan: {error}")),
    }
}

impl CallableTrait for WhoamiCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let creds = CredentialsManager::with_default_store().load()?;
        for line in describe_saved_login(creds.as_ref()) {
            println!("{}", line);
        }

        println!();
        for line in load_subscription_plan(creds.as_ref()) {
            println!("{}", line);
        }

        let project_dir = std::env::current_dir()?;
        let project_lock = load_project_lock(&project_dir)?;
        println!();
        for line in describe_project_lock(project_lock.as_ref()) {
            println!("{}", line);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn describe_saved_login_marks_expired_credentials() {
        let creds = StoredCredentials {
            access_token: "token".to_string(),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            expires_at: Utc::now() - Duration::minutes(1),
            email: Some("user@example.com".to_string()),
            server_url: Some("https://stacker.example".to_string()),
            org: Some("demo".to_string()),
            domain: None,
        };

        let lines = describe_saved_login(Some(&creds));
        assert!(lines[0].contains("user@example.com"));
        assert!(lines[0].contains("(expired)"));
        assert!(lines
            .iter()
            .any(|line| line.contains("https://stacker.example")));
    }

    #[test]
    fn describe_project_lock_shows_recorded_deployer() {
        let lock = DeploymentLock::for_local()
            .with_project_name(Some("demo".into()))
            .with_stacker_email(Some("owner@example.com".into()));

        let lines = describe_project_lock(Some(&lock));
        assert!(lines.iter().any(|line| line.contains("owner@example.com")));
        assert!(lines.iter().any(|line| line.contains("demo")));
    }

    #[test]
    fn load_subscription_plan_reports_missing_login() {
        let lines = load_subscription_plan(None);

        assert_eq!(lines[0], "Subscription: unavailable");
        assert!(lines.iter().any(|line| line.contains("not logged in")));
    }
}
