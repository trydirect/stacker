//! Cloud-init user-data render for the immutable-deploy model.
//!
//! In the baked-server model the ONLY per-user variance is what gets injected at
//! first boot. This produces the `#cloud-config` user-data passed to
//! `HetznerCloudConnector::create_server_from_image`: it writes the per-user env
//! file, renders the domain vhost, and (re)starts the baked compose. It is a
//! pure, deterministic function of [`BootConfig`] — no I/O — so it is fully
//! unit-testable and can't fail at deploy time.
//!
//! CONTRACT: the file paths, permissions, env-file format, and the
//! `stacker-compose.service` unit name here are the DEPLOY side of the canonical
//! boot contract at `config/shared-fixtures/immutable-deploy/boot-contract.json`.
//! The BAKE side (install service) builds a snapshot expecting this exact layout.
//! Changing any of them is a breaking change — update the contract + both
//! services together.

use std::collections::BTreeMap;

use serde_json::json;

/// Everything that differs between two deploys of the same baked snapshot.
/// Secrets are already resolved (e.g. fetched from Vault) into `env`.
#[derive(Debug, Clone, Default)]
pub struct BootConfig {
    pub domain: String,
    pub admin_email: String,
    /// Per-user environment (KEY -> value). BTreeMap → deterministic ordering.
    pub env: BTreeMap<String, String>,
}

/// Render the cloud-init `#cloud-config` user-data for a baked-snapshot boot.
///
/// Built via `serde_yaml` (not string concat) so indentation/escaping are always
/// correct and the output is deterministic.
pub fn render_user_data(cfg: &BootConfig) -> String {
    let env_content = env_file(&cfg.env);
    let vhost = nginx_vhost(&cfg.domain);

    let doc = json!({
        "write_files": [
            {
                "path": "/etc/stacker/env",
                "permissions": "0600",
                "owner": "root:root",
                "content": env_content,
            },
            {
                "path": "/etc/stacker/app.domain.conf",
                "permissions": "0644",
                "content": vhost,
            },
        ],
        // The compose file, images, and systemd unit are all baked into the
        // snapshot — first boot just (re)starts them with the injected env.
        "runcmd": [
            ["systemctl", "restart", "stacker-compose.service"],
        ],
    });

    let yaml =
        serde_yaml::to_string(&doc).unwrap_or_else(|_| "write_files: []\nruncmd: []\n".to_string());
    format!("#cloud-config\n{yaml}")
}

/// `KEY=value` lines, deterministically ordered.
fn env_file(env: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (k, v) in env {
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    out
}

/// Minimal, deterministic nginx vhost for the user's domain. Kept intentionally
/// tiny — the reverse proxy / TLS specifics live in the baked image.
fn nginx_vhost(domain: &str) -> String {
    format!(
        "server {{\n    server_name {domain};\n    location / {{\n        proxy_pass http://127.0.0.1:8080;\n        proxy_set_header Host $host;\n    }}\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BootConfig {
        let mut env = BTreeMap::new();
        env.insert("SECRET_KEY".to_string(), "s3cr3t".to_string());
        env.insert("DB_PASSWORD".to_string(), "pw123".to_string());
        env.insert("DOMAIN".to_string(), "app.example.com".to_string());
        BootConfig {
            domain: "app.example.com".to_string(),
            admin_email: "admin@example.com".to_string(),
            env,
        }
    }

    #[test]
    fn starts_with_cloud_config_header() {
        assert!(render_user_data(&sample()).starts_with("#cloud-config\n"));
    }

    #[test]
    fn includes_every_env_var_as_key_value() {
        let out = render_user_data(&sample());
        // The env file content is embedded; assert each pair is present.
        assert!(out.contains("DB_PASSWORD=pw123"), "got: {out}");
        assert!(out.contains("SECRET_KEY=s3cr3t"), "got: {out}");
        assert!(out.contains("DOMAIN=app.example.com"), "got: {out}");
    }

    #[test]
    fn renders_domain_into_the_vhost() {
        let out = render_user_data(&sample());
        assert!(out.contains("server_name app.example.com"), "got: {out}");
    }

    #[test]
    fn restarts_the_baked_compose_unit() {
        assert!(render_user_data(&sample()).contains("stacker-compose.service"));
    }

    #[test]
    fn is_deterministic() {
        // Same input -> byte-identical output (BTreeMap ordering).
        assert_eq!(render_user_data(&sample()), render_user_data(&sample()));
    }

    #[test]
    fn env_file_is_sorted_and_newline_terminated() {
        let mut env = BTreeMap::new();
        env.insert("B".into(), "2".into());
        env.insert("A".into(), "1".into());
        assert_eq!(env_file(&env), "A=1\nB=2\n");
    }
}
