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
    /// `mutability: generated` fields to (re)generate on the box at first boot,
    /// as `(env_key, shell_generator_expr)`. Each runs before the compose
    /// restart and overwrites (or appends) the key in `/etc/stacker/env`, so a
    /// clone gets a fresh secret instead of the one frozen into the snapshot.
    /// The generator strings come from the same source of truth as a normal
    /// install's `generate-secrets.sh`.
    pub regen: Vec<(String, String)>,
    /// `type: derived_jwt` fields to mint on the box AFTER `regen` runs (so the
    /// signing field it depends on is already written to `/etc/stacker/env`).
    /// The header/claims are precomputed to base64url here; only the HMAC
    /// signature is done on the box (it depends on the runtime signing value).
    pub regen_jwt: Vec<DerivedJwtSpec>,
}

/// A `type: derived_jwt` field to sign on the cloned box. `header_b64`/
/// `payload_b64` are precomputed base64url (so no author data hits the shell);
/// the box only HMACs `header.payload` with the runtime value of
/// `signing_env_key` read from `/etc/stacker/env`.
#[derive(Debug, Clone)]
pub struct DerivedJwtSpec {
    /// Env var that receives the signed JWT.
    pub target_key: String,
    /// Env var holding the signing secret (the `signing_key` field, resolved).
    pub signing_env_key: String,
    /// Precomputed base64url of `{"alg":..,"typ":"JWT"}`.
    pub header_b64: String,
    /// Precomputed base64url of the compact claims JSON.
    pub payload_b64: String,
    /// openssl digest flag matching `alg`: `sha256` | `sha384` | `sha512`.
    pub openssl_dgst: String,
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
        // snapshot — first boot regenerates any per-install secrets, then
        // (re)starts the stack with the injected env.
        "runcmd": regen_runcmds(&cfg.regen, &cfg.regen_jwt),
    });

    let yaml =
        serde_yaml::to_string(&doc).unwrap_or_else(|_| "write_files: []\nruncmd: []\n".to_string());
    format!("#cloud-config\n{yaml}")
}

/// The cloud-init `runcmd` list: one regeneration command per generated field
/// (each mints a fresh value on the box and writes it into `/etc/stacker/env`),
/// then the `derived_jwt` fields (which read their now-written signing field),
/// then the compose restart that reads that env.
///
/// Keys are restricted to env-identifier characters; anything else is dropped
/// rather than interpolated into the shell.
fn regen_runcmds(
    regen: &[(String, String)],
    regen_jwt: &[DerivedJwtSpec],
) -> Vec<serde_json::Value> {
    fn valid_key(k: &str) -> bool {
        !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    let mut cmds: Vec<serde_json::Value> = Vec::new();
    for (key, expr) in regen {
        if !valid_key(key) {
            continue;
        }
        // Compute the value once, then replace an existing line in place or
        // append it. `#` as the sed delimiter avoids clashing with `/` in
        // base64 output; the generated types never contain `#`.
        let snippet = format!(
            "V=$({expr}); if grep -q '^{key}=' /etc/stacker/env 2>/dev/null; then \
             sed -i \"s#^{key}=.*#{key}=${{V}}#\" /etc/stacker/env; else \
             echo \"{key}=${{V}}\" >> /etc/stacker/env; fi"
        );
        cmds.push(json!(["bash", "-lc", snippet]));
    }

    // derived_jwt: sign header.payload (precomputed base64url) with the runtime
    // value of the signing field, which the loop above has already written.
    for spec in regen_jwt {
        if !valid_key(&spec.target_key) || !valid_key(&spec.signing_env_key) {
            continue;
        }
        if !matches!(spec.openssl_dgst.as_str(), "sha256" | "sha384" | "sha512") {
            continue;
        }
        let DerivedJwtSpec {
            target_key,
            signing_env_key,
            header_b64,
            payload_b64,
            openssl_dgst,
        } = spec;
        let snippet = format!(
            "SK=$(grep '^{signing_env_key}=' /etc/stacker/env 2>/dev/null | cut -d= -f2-); \
             SIG=$(printf '%s' \"{header_b64}.{payload_b64}\" | \
             openssl dgst -{openssl_dgst} -hmac \"$SK\" -binary | \
             openssl base64 -A | tr '+/' '-_' | tr -d '='); \
             JWT=\"{header_b64}.{payload_b64}.${{SIG}}\"; \
             if grep -q '^{target_key}=' /etc/stacker/env 2>/dev/null; then \
             sed -i \"s#^{target_key}=.*#{target_key}=${{JWT}}#\" /etc/stacker/env; else \
             echo \"{target_key}=${{JWT}}\" >> /etc/stacker/env; fi"
        );
        cmds.push(json!(["bash", "-lc", snippet]));
    }

    cmds.push(json!(["systemctl", "restart", "stacker-compose.service"]));
    cmds
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
            regen: Vec::new(),
            regen_jwt: Vec::new(),
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
