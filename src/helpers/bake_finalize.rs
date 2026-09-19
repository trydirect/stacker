//! Finalize a build box before it is snapshotted (immutable-deploy BAKE step).
//!
//! A bake snapshots the *whole disk* of a build box that the author deployed
//! normally. That disk carries three classes of the author's own secrets, none
//! of which a buyer's clone should inherit:
//!
//! 1. **Machine identity** — SSH host keys and `machine-id`. Left in place,
//!    every clone of the image shares them, so any buyer can impersonate the
//!    SSH host of any other buyer. This is the standard "sysprep" step every
//!    golden-image pipeline performs (`virt-sysprep`, Packer, the DigitalOcean
//!    1-Click checklist); we had none.
//! 2. **The co-located `.env`** — shipped verbatim next to the compose file by
//!    the deploy bundle, so the author's literal secrets sit in the image even
//!    when the compose itself is fully parameterized. Parameterizing compose
//!    alone just moves the secret from one file in the image to another.
//! 3. **Initialized data volumes** — a secret the app wrote into its own
//!    database/volume on first run is frozen in the snapshot and is *not*
//!    governed by environment variables any more. Postgres is the canonical
//!    case: `POSTGRES_PASSWORD` is honoured only when it initializes an empty
//!    data directory, so a cloned box silently keeps the author's role password.
//!
//! Everything here is a pure function producing shell, so the policy is
//! unit-tested without infra; [`crate::helpers::bake`] wires it to a real SSH
//! session.

use std::collections::BTreeSet;

/// Volumes whose content must survive the bake, keyed by stack slug.
///
/// Defaulting to *reset* is deliberate: wrongly resetting a volume costs a
/// rebuild of cheap state, while wrongly keeping one leaks the author's
/// credentials to every buyer. Only volumes that are expensive to rebuild
/// **and** carry no credentials belong here.
///
/// `stackpilot`'s Ollama volume holds the pulled model weights — gigabytes,
/// with a 600s pull timeout in `scripts/download-model.sh`. Preserving it is
/// the entire economic point of baking that stack.
pub fn volumes_to_keep(stack: &str) -> &'static [&'static str] {
    match stack {
        "stackpilot" => &["ollama"],
        _ => &[],
    }
}

/// Shell to strip machine identity so each clone boots as a distinct host.
///
/// `sshd` regenerates host keys on first boot when none are present, and
/// `systemd` repopulates an empty `/etc/machine-id`; clearing cloud-init's
/// instance state makes it treat the clone as a new instance and re-run its
/// per-instance modules.
pub fn identity_reset_commands() -> Vec<String> {
    vec![
        "rm -f /etc/ssh/ssh_host_*".to_string(),
        ": > /etc/machine-id".to_string(),
        "rm -f /var/lib/dbus/machine-id".to_string(),
        "rm -rf /var/lib/cloud/instances /var/lib/cloud/instance".to_string(),
        "rm -f /root/.bash_history /home/*/.bash_history".to_string(),
        "find /var/log -type f -exec truncate -s 0 {} + 2>/dev/null || true".to_string(),
    ]
}

/// Shell to stop the stack and drop every data volume that is not explicitly
/// preserved, so the buyer's box initializes them from scratch with the
/// buyer's own generated values.
///
/// Volume names are matched by suffix because Compose prefixes them with the
/// project name (`project_stackpilot_pgdata` for a declared `stackpilot_pgdata`).
pub fn volume_reset_commands(project_dir: &str, keep: &[&str]) -> Vec<String> {
    let mut cmds = vec![format!(
        "cd {project_dir} && docker compose down --remove-orphans"
    )];

    let filter = if keep.is_empty() {
        "cat".to_string()
    } else {
        let pattern = keep.join("|");
        format!("grep -Ev '({pattern})'")
    };

    cmds.push(format!(
        "docker volume ls -q | {filter} | xargs -r docker volume rm -f"
    ));
    cmds
}

/// Blank the values of secret-bearing keys in a `.env` file while keeping the
/// file's shape: keys, comments, blank lines and non-secret values survive.
///
/// The keys are kept (rather than the lines dropped) so the file still
/// documents what the stack expects; on the buyer's box the whole file is
/// replaced from `/etc/stacker/env` by the systemd unit's `ExecStartPre`, so
/// the blanked values are never read.
pub fn scrub_env_file(content: &str, protected: &BTreeSet<String>) -> String {
    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        match line.split_once('=') {
            Some((key, _value)) if should_blank(key.trim(), protected) => {
                out.push_str(key);
                out.push_str("=\n");
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }

    out
}

/// A key is blanked when the author's contract declared it regenerable/buyer-
/// supplied, or when its name is secret-shaped by the same heuristic the CLI
/// already uses for `generate-secrets.sh`.
fn should_blank(key: &str, protected: &BTreeSet<String>) -> bool {
    protected.contains(key) || crate::console::commands::cli::init::is_secret_env_key(key)
}

/// Everything the finalize step needs to reach and sanitize a build box.
#[derive(Debug, Clone)]
pub struct FinalizeContext {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub private_key_pem: String,
    /// Where the deploy put the compose file and its co-located `.env`.
    pub project_dir: String,
    /// Stack slug — selects the volume keep-list.
    pub stack: String,
    /// Contract fields with `mutability: generated`/`provided`.
    pub protected_keys: BTreeSet<String>,
}

/// What the finalize step learned about the image it just sanitized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizeOutcome {
    /// `${VAR}` names the sanitized compose references — pinned to the snapshot
    /// so the clone path can fail closed on an environment that cannot satisfy
    /// them.
    pub required_env_keys: BTreeSet<String>,
}

/// Sanitize a build box in place, immediately before it is snapshotted.
///
/// Order matters: the compose/env rewrite has to happen while the stack is
/// still described on disk, the volume reset tears the stack down, and the
/// identity reset goes last because it leaves the box unable to present a
/// stable SSH identity afterwards. The build box is throwaway, so none of this
/// needs to be reversible.
pub async fn finalize_build_box(
    ctx: &FinalizeContext,
) -> Result<FinalizeOutcome, crate::helpers::bake::BakeError> {
    use crate::helpers::bake::BakeError;
    use crate::helpers::ssh_client::{disconnect_ssh, exec_remote, open_ssh};

    let fail = |stage: &str, err: String| BakeError::Finalize(format!("{stage}: {err}"));

    let session = open_ssh(
        &ctx.host,
        ctx.port,
        &ctx.user,
        &ctx.private_key_pem,
        std::time::Duration::from_secs(30),
    )
    .await
    .map_err(|e| fail("ssh connect", e.to_string()))?;

    let run = |cmd: String| {
        let session = &session;
        async move {
            let (stdout, stderr, code) = exec_remote(session, &cmd, 300)
                .await
                .map_err(|e| e.to_string())?;
            if code != 0 {
                return Err(format!("`{cmd}` exited {code}: {stderr}"));
            }
            Ok::<String, String>(stdout)
        }
    };

    let compose_path = format!("{}/docker-compose.yml", ctx.project_dir);
    let env_path = format!("{}/.env", ctx.project_dir);

    let result = async {
        // 1. Read what the deploy left on the box.
        let compose = run(format!("cat {compose_path}"))
            .await
            .map_err(|e| fail("read compose", e))?;
        // A stack may legitimately have no .env; treat that as empty.
        let env_raw = run(format!("cat {env_path} 2>/dev/null || true"))
            .await
            .map_err(|e| fail("read .env", e))?;
        let env_values = parse_env_pairs(&env_raw);

        // 2. Replace secrets embedded inside larger values (DSNs) with ${KEY}
        //    references. Whole-value keys were already parameterized at deploy
        //    time by `parameterize_compose_env_vars`.
        let sanitized = crate::cli::generator::compose::parameterize_embedded_secret_values(
            &compose,
            &env_values,
            &ctx.protected_keys,
        )
        .map_err(|conflict| BakeError::Finalize(conflict.to_string()))?;

        if sanitized != compose {
            write_remote_file(&run, &compose_path, &sanitized)
                .await
                .map_err(|e| fail("write compose", e))?;
        }

        // 3. Record what the image now needs from the buyer's env file.
        let required_env_keys =
            crate::cli::generator::compose::collect_env_var_references(&sanitized);

        // 4. Blank the author's secrets in the co-located .env. The buyer's box
        //    overwrites this file wholesale from /etc/stacker/env at boot, so
        //    the blanked values are never read.
        if !env_raw.trim().is_empty() {
            let scrubbed = scrub_env_file(&env_raw, &ctx.protected_keys);
            write_remote_file(&run, &env_path, &scrubbed)
                .await
                .map_err(|e| fail("write .env", e))?;
        }

        // 5. Drop initialized data volumes so the buyer's box re-initializes
        //    them with the buyer's own values. A secret the app persisted on
        //    first run is not governed by env vars any more.
        for cmd in volume_reset_commands(&ctx.project_dir, volumes_to_keep(&ctx.stack)) {
            run(cmd).await.map_err(|e| fail("volume reset", e))?;
        }

        // 6. Strip machine identity last.
        for cmd in identity_reset_commands() {
            run(cmd).await.map_err(|e| fail("identity reset", e))?;
        }

        Ok(FinalizeOutcome { required_env_keys })
    }
    .await;

    disconnect_ssh(session).await;
    result
}

/// Write `content` to `path` on the remote box without any quoting hazards:
/// the payload travels base64-encoded and is decoded on the far side.
async fn write_remote_file<F, Fut>(run: &F, path: &str, content: &str) -> Result<(), String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let encoded = STANDARD.encode(content.as_bytes());
    run(format!("printf %s {encoded} | base64 -d > {path}"))
        .await
        .map(|_| ())
}

/// Parse `KEY=value` lines into a map, skipping comments and blanks.
pub fn parse_env_pairs(content: &str) -> std::collections::BTreeMap<String, String> {
    let mut pairs = std::collections::BTreeMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let value = value.trim();
            if !value.is_empty() {
                pairs.insert(key.trim().to_string(), value.to_string());
            }
        }
    }

    pairs
}

/// The contract fields a buyer's box regenerates or supplies — i.e. the ones a
/// `${KEY}` reference can safely point at.
///
/// Mirrors the selection `compose_env_keys` makes at deploy time
/// (`src/console/commands/cli/deploy.rs`), reading the contract as raw JSON so
/// an unparseable or partially-shaped contract degrades to "nothing protected"
/// rather than failing the bake outright.
pub fn protected_keys_from_contract(contract: &serde_json::Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();

    let Some(services) = contract.get("services").and_then(|v| v.as_object()) else {
        return keys;
    };

    for service in services.values() {
        let Some(fields) = service.get("fields").and_then(|v| v.as_object()) else {
            continue;
        };
        for (name, policy) in fields {
            let protected = matches!(
                policy.get("mutability").and_then(|v| v.as_str()),
                Some("generated") | Some("provided")
            );
            if protected {
                keys.insert(name.clone());
            }
        }
    }

    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected(keys: &[&str]) -> BTreeSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn scrub_blanks_contract_declared_keys() {
        let env = "SECRET_KEY=b838f1f2\nOLLAMA_MODEL=llama3.1\n";
        let out = scrub_env_file(env, &protected(["SECRET_KEY"].as_slice()));
        assert!(out.contains("SECRET_KEY=\n"), "blanked:\n{out}");
        assert!(
            out.contains("OLLAMA_MODEL=llama3.1"),
            "non-secret kept:\n{out}"
        );
    }

    #[test]
    fn scrub_blanks_secret_shaped_keys_without_a_contract() {
        // No contract at all — the name heuristic still has to catch these.
        let env = "DB_PASSWORD=2213a996\nADMIN_USER=admin\n";
        let out = scrub_env_file(env, &BTreeSet::new());
        assert!(out.contains("DB_PASSWORD=\n"), "blanked:\n{out}");
        assert!(out.contains("ADMIN_USER=admin"), "non-secret kept:\n{out}");
    }

    #[test]
    fn scrub_preserves_comments_and_blank_lines() {
        let env = "# Secrets\n\nSECRET_KEY=abc\n";
        let out = scrub_env_file(env, &BTreeSet::new());
        assert_eq!(out, "# Secrets\n\nSECRET_KEY=\n");
    }

    #[test]
    fn scrub_keeps_values_containing_equals_signs() {
        // A blanked key must not be confused by '=' inside a kept value.
        let env = "OLLAMA_MODEL=llama3.1\nJWT_SECRET=a=b=c\n";
        let out = scrub_env_file(env, &BTreeSet::new());
        assert!(out.contains("OLLAMA_MODEL=llama3.1"), "kept:\n{out}");
        assert!(out.contains("JWT_SECRET=\n"), "blanked:\n{out}");
    }

    #[test]
    fn identity_reset_covers_host_keys_machine_id_and_cloud_init() {
        let cmds = identity_reset_commands().join(" ; ");
        assert!(cmds.contains("/etc/ssh/ssh_host_*"), "host keys: {cmds}");
        assert!(cmds.contains("/etc/machine-id"), "machine-id: {cmds}");
        assert!(
            cmds.contains("/var/lib/cloud/instance"),
            "cloud-init: {cmds}"
        );
    }

    #[test]
    fn volume_reset_preserves_the_kept_volumes() {
        let cmds = volume_reset_commands("/home/trydirect/project", &["ollama"]).join(" ; ");
        assert!(
            cmds.contains("docker compose down"),
            "stack stopped: {cmds}"
        );
        assert!(
            cmds.contains("grep -Ev '(ollama)'"),
            "kept volume excluded from removal: {cmds}"
        );
    }

    #[test]
    fn volume_reset_without_a_keep_list_removes_everything() {
        let cmds = volume_reset_commands("/home/trydirect/project", &[]).join(" ; ");
        assert!(cmds.contains("| cat |"), "no filter applied: {cmds}");
    }

    #[test]
    fn parse_env_pairs_skips_comments_blanks_and_empty_values() {
        let env = "# header\n\nA=1\nEMPTY=\nB=two\n";
        let pairs = parse_env_pairs(env);
        assert_eq!(pairs.get("A").map(String::as_str), Some("1"));
        assert_eq!(pairs.get("B").map(String::as_str), Some("two"));
        assert!(!pairs.contains_key("EMPTY"), "empty value is not a secret");
    }

    #[test]
    fn protected_keys_cover_generated_and_provided_only() {
        let contract = serde_json::json!({
            "services": {
                "db": { "fields": {
                    "POSTGRES_PASSWORD": { "mutability": "generated" },
                    "POSTGRES_USER": { "mutability": "fixed" }
                }},
                "app": { "fields": {
                    "LICENSE_KEY": { "mutability": "provided" },
                    "LOG_LEVEL": { "mutability": "editable" }
                }}
            }
        });
        let keys = protected_keys_from_contract(&contract);
        assert!(keys.contains("POSTGRES_PASSWORD"));
        assert!(keys.contains("LICENSE_KEY"));
        assert!(!keys.contains("POSTGRES_USER"), "fixed is not regenerated");
        assert!(!keys.contains("LOG_LEVEL"), "editable is not regenerated");
    }

    #[test]
    fn protected_keys_from_a_shapeless_contract_is_empty() {
        assert!(protected_keys_from_contract(&serde_json::Value::Null).is_empty());
    }

    #[test]
    fn stackpilot_keeps_only_its_model_volume() {
        assert_eq!(volumes_to_keep("stackpilot"), &["ollama"]);
        // An unknown stack defaults to resetting everything — leaking is worse
        // than rebuilding.
        assert!(volumes_to_keep("something-else").is_empty());
    }
}
