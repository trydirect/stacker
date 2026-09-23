//! `bake` — turn a deployed build box into a published snapshot (immutable
//! deploy, slice 2). You deploy a stack onto a throwaway Hetzner box the normal
//! way (`stacker install lamp` onto a fresh server), then run this to
//! health-gate it, snapshot it, and record the BakeRecord in the snapshot
//! registry (so `POST /api/v1/deploy/clone` can resolve the image_id).
//!
//! Usage:
//!   HETZNER_TOKEN=... DATABASE_URL=... cargo run --bin bake -- \
//!     --ip <build-box-ip> --stack ai-workflows-v2 --version 1.0.0 \
//!     --health-url http://<ip>/health --ssh-key ~/.ssh/id_ed25519
//!
//! `--ssh-key` is required: before snapshotting we sanitize the build box
//! (strip machine identity, blank the author's `.env`, parameterize secrets
//! embedded in compose values, drop initialized data volumes). Without it the
//! image would carry the author's credentials to every buyer, so the bake is
//! refused unless `--allow-unsanitized-snapshot` is passed deliberately.
//!
//! `DATABASE_URL` (the stacker Postgres) persists the BakeRecord; without it
//! the bake still snapshots and prints the record, but it is not registered.

use stacker::connectors::hetzner::{HetznerCloudClient, HetznerSnapshotTarget};
use stacker::helpers::bake::run_bake;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut ip: Option<String> = None;
    let mut server_id: Option<i64> = None;
    let mut stack = "lamp".to_string();
    let mut version = "v1".to_string();
    let mut health_url: Option<String> = None;
    let mut ssh_key: Option<String> = None;
    let mut ssh_user = "root".to_string();
    let mut project_dir = "/home/trydirect/project".to_string();
    let mut allow_unsanitized = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--ip" => {
                ip = args.get(i + 1).cloned();
                i += 2;
            }
            "--server-id" => {
                server_id = args.get(i + 1).and_then(|v| v.parse().ok());
                i += 2;
            }
            "--stack" => {
                stack = args.get(i + 1).cloned().unwrap_or(stack);
                i += 2;
            }
            "--version" => {
                version = args.get(i + 1).cloned().unwrap_or(version);
                i += 2;
            }
            "--health-url" => {
                health_url = args.get(i + 1).cloned();
                i += 2;
            }
            "--ssh-key" => {
                ssh_key = args.get(i + 1).cloned();
                i += 2;
            }
            "--ssh-user" => {
                ssh_user = args.get(i + 1).cloned().unwrap_or(ssh_user);
                i += 2;
            }
            "--project-dir" => {
                project_dir = args.get(i + 1).cloned().unwrap_or(project_dir);
                i += 2;
            }
            "--allow-unsanitized-snapshot" => {
                allow_unsanitized = true;
                i += 1;
            }
            other => {
                // Shrugging this off is how `--sshkey` silently became "no
                // --ssh-key", and a mistyped `--stack` silently bakes under the
                // default slug — pinning the wrong contract to the image.
                return Err(format!(
                    "unknown argument `{other}`. Supported: --ip, --server-id, --stack, \
                     --version, --health-url, --ssh-key, --ssh-user, --project-dir, \
                     --allow-unsanitized-snapshot"
                )
                .into());
            }
        }
    }

    if ip.is_none() && server_id.is_none() {
        return Err("provide --ip <build-box-ip> or --server-id <id>".into());
    }

    let token = std::env::var("HETZNER_TOKEN")
        .map_err(|_| "set HETZNER_TOKEN to the Hetzner Cloud API token".to_string())?;

    // Health gate.
    let (healthy, detail) = match &health_url {
        Some(url) => match reqwest::get(url).await {
            Ok(r) if r.status().is_success() => (true, String::new()),
            Ok(r) => (false, format!("{url} returned {}", r.status())),
            Err(e) => (false, format!("{url} unreachable: {e}")),
        },
        None => {
            eprintln!("WARNING: no --health-url given; skipping health gate (assuming healthy)");
            (true, String::new())
        }
    };

    let target = HetznerSnapshotTarget {
        provider_server_id: server_id,
        server_name: None,
        public_ip: ip.clone(),
    };

    // Resolve the author's field policy *before* finalizing: it decides which
    // keys get blanked in `.env` and which names an embedded secret may be
    // parameterized to. Also pinned to the snapshot further down so the clone
    // path can regenerate those fields per buyer.
    let pool = match std::env::var("DATABASE_URL") {
        Ok(db_url) => Some(sqlx::PgPool::connect(&db_url).await?),
        Err(_) => {
            eprintln!("WARNING: DATABASE_URL not set — bake will NOT be registered in the snapshot registry.");
            None
        }
    };

    let config_contract = match &pool {
        Some(pool) => resolve_config_contract(pool, &stack, &version).await,
        None => None,
    };
    let protected_keys = config_contract
        .as_ref()
        .map(stacker::helpers::bake_finalize::protected_keys_from_contract)
        .unwrap_or_default();

    // Parsed form: the volume declarations are per-service, which the flat key
    // set above has thrown away.
    //
    // A contract that fails to parse aborts. Treating it as absent would be
    // worse than it sounds: `protected_keys` above is derived from the raw JSON
    // and would still be non-empty, so `check_contract_usable` passes while
    // finalize sanitizes against an empty contract — an unsanitized image,
    // published with "Sanitized" in the log. Reachable in practice: every
    // contract type denies unknown fields, so a template using a newer kind
    // fails wholesale on an older bake binary.
    let parsed_contract: stacker::cli::config_parser::ConfigContract = match &config_contract {
        Some(value) => serde_json::from_value(value.clone()).map_err(|e| {
            format!(
                "the config_contract stored for '{stack}' could not be parsed: {e}. \
                 Refusing rather than baking an image nothing was sanitized against. \
                 If the template uses a newer contract feature, rebuild this binary."
            )
        })?,
        None => Default::default(),
    };

    // Refuse before touching the box: with no contract there is nothing to
    // sanitize, and publishing anyway is how the author's credentials reach
    // every buyer.
    stacker::helpers::bake_finalize::check_contract_usable(&protected_keys, allow_unsanitized)
        .map_err(|e| e.to_string())?;

    // Sanitize the build box before the snapshot is taken.
    let finalize_outcome = match (&ssh_key, allow_unsanitized) {
        (Some(key_path), _) => {
            let Some(host) = ip.clone() else {
                return Err("--ssh-key needs --ip (the build box address to connect to)".into());
            };
            let private_key_pem = std::fs::read_to_string(key_path)
                .map_err(|e| format!("could not read --ssh-key {key_path}: {e}"))?;

            eprintln!("==> Finalizing build box before snapshot...");
            let ctx = stacker::helpers::bake_finalize::FinalizeContext {
                host,
                port: 22,
                user: ssh_user.clone(),
                private_key_pem,
                project_dir: project_dir.clone(),
                stack: stack.clone(),
                contract: parsed_contract.clone(),
                protected_keys: protected_keys.clone(),
            };
            let outcome = stacker::helpers::bake_finalize::finalize_build_box(&ctx).await?;
            eprintln!(
                "  Sanitized. Compose requires {} env key(s): {}",
                outcome.required_env_keys.len(),
                outcome
                    .required_env_keys
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Some(outcome)
        }
        (None, true) => {
            eprintln!(
                "WARNING: --allow-unsanitized-snapshot set. The image will keep the author's \
                 .env values, data volumes and SSH host keys. Do NOT publish it to buyers."
            );
            None
        }
        (None, false) => {
            return Err(
                "--ssh-key is required so the build box can be sanitized before \
                        snapshotting (pass --allow-unsanitized-snapshot to skip, for a \
                        private image only)"
                    .into(),
            )
        }
    };

    let connector = HetznerCloudClient::from_env().map_err(|e| e.to_string())?;
    let record = run_bake(
        &connector, &token, target, &stack, &version, healthy, &detail,
    )
    .await
    .map_err(|e| e.to_string())?;

    println!("{}", serde_json::to_string_pretty(&record)?);
    eprintln!(
        "\nBaked {}:{} -> Hetzner image_id={}. Clone it with create_server_from_image(image={}).",
        record.stack, record.version, record.image_id, record.image_id
    );

    // Persist into the snapshot registry so /api/v1/deploy/clone can resolve it.
    if let Some(pool) = pool {
        let required_env_keys = finalize_outcome.as_ref().map(|outcome| {
            serde_json::Value::Array(
                outcome
                    .required_env_keys
                    .iter()
                    .map(|key| serde_json::Value::String(key.clone()))
                    .collect(),
            )
        });

        let row = stacker::db::baked_snapshot::record(
            &pool,
            &record.stack,
            &record.version,
            &record.provider,
            record.image_id,
            record.healthy,
            None,
            config_contract,
            required_env_keys,
        )
        .await
        .map_err(|e| e.to_string())?;
        eprintln!(
            "Registered snapshot in registry: id={} stack={}:{} image_id={}",
            row.id, row.stack, row.version, row.image_id
        );
    }

    Ok(())
}

/// The author's field policy for `stack`, resolved by the same slug the
/// snapshot registry keys on (`baked_snapshots.stack == stack_template.slug`).
///
/// `get_config_contract` reads the template's *latest* version. Baking some
/// other version would pin the wrong field set to the image, so the versions
/// are compared and a mismatch stops the bake rather than shipping a snapshot
/// whose contract describes a different release.
async fn resolve_config_contract(
    pool: &sqlx::PgPool,
    stack: &str,
    version: &str,
) -> Option<serde_json::Value> {
    match stacker::db::marketplace::get_approved_by_slug(pool, stack).await {
        Ok(Some(template)) => {
            match stacker::db::marketplace::get_by_slug_with_latest(pool, stack).await {
                Ok((_, Some(latest))) if latest.version != version => {
                    eprintln!(
                        "WARNING: baking '{stack}' v{version}, but the marketplace's latest \
                         version is v{}. The contract describes the latest version, so it \
                         would not match this image — resubmit or bake the latest version.",
                        latest.version
                    );
                    return None;
                }
                _ => {}
            }
            match stacker::db::marketplace::get_config_contract(pool, template.id).await {
                Ok(serde_json::Value::Null) => {
                    eprintln!("WARNING: template '{stack}' declares no config_contract — nothing will be regenerated per buyer.");
                    None
                }
                Ok(contract) => Some(contract),
                Err(err) => {
                    eprintln!("WARNING: could not read config_contract for '{stack}': {err}");
                    None
                }
            }
        }
        Ok(None) => {
            eprintln!("WARNING: no approved template found for stack '{stack}'.");
            None
        }
        Err(err) => {
            eprintln!("WARNING: could not resolve template for '{stack}': {err}");
            None
        }
    }
}
