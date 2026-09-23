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

/// Volumes the author declared `mutability: fixed` — the ones whose content
/// travels into the image.
///
/// Everything else resets, declared or not. The error direction is deliberate:
/// forgetting a declaration costs a rebuild of cheap state, while keeping a
/// credential-bearing volume hands the author's secrets to every buyer.
///
/// This used to be a `match` on the stack slug in this file. The author knows
/// which of their volumes are expensive and which hold credentials; the platform
/// does not, and a hardcoded list needed a code change and a rebuilt binary for
/// every new stack.
pub fn volumes_to_keep(contract: &crate::cli::config_parser::ConfigContract) -> Vec<String> {
    contract
        .services
        .values()
        .flat_map(|service| service.fixed_volumes())
        .collect()
}

/// Validate the author's volume declarations.
///
/// Only what a machine can actually establish is checked here: that the name is
/// safe to interpolate into a shell `case` pattern. Whether a volume is *safe to
/// keep* is the author's call, and deliberately so.
///
/// An earlier version refused any volume belonging to a service that declares
/// `generated` or `provided` fields, on the theory that such a service wrote the
/// secret into its own data. Measurement killed that rule: a Postgres data
/// directory holds `SCRAM-SHA-256$4096:…`, a hash — the password appears nowhere,
/// literally or base64-encoded; n8n keeps its own encryption key in
/// `database.sqlite`; and a Qdrant volume holds nothing but collections, because
/// Qdrant reads its API key from the environment on every start. Searching the
/// volume for the secret finds nothing in any of the three, so that cannot
/// distinguish them either.
///
/// The real difference is behavioural — whether the service derives persistent
/// state from the secret — and it is not visible in the volume's bytes. The
/// author knows it; the platform cannot compute it. So the platform checks what
/// it can and trusts the author with the rest, the same way `config_contract`
/// trusts the author about which fields are sensitive.
pub fn check_volume_declarations(
    contract: &crate::cli::config_parser::ConfigContract,
) -> Result<(), crate::helpers::bake::BakeError> {
    for (service_name, service) in &contract.services {
        for volume in service.fixed_volumes() {
            if !is_plain_volume_name(&volume) {
                return Err(crate::helpers::bake::BakeError::Finalize(format!(
                    "volume `{volume}` on service `{service_name}` contains characters \
                     that are shell pattern syntax. It would match something other than \
                     intended, and keeping a volume that should have been reset leaves \
                     the author's credentials in the image. Use only letters, digits, \
                     `_`, `-` and `.`."
                )));
            }
        }
    }

    Ok(())
}

/// Values a service would lose when the buyer's machine replaces the env file.
///
/// A service declaring `env_file:` reads its values out of that file rather than
/// through `${VAR}` in the compose. At first boot the buyer's machine overwrites
/// the file wholesale from `/etc/stacker/env`, which carries only contract
/// fields and the buyer's own values — so every other key in it disappears, and
/// the container starts without values it had on the build box.
///
/// Returns the keys that would be lost, in sorted order. Empty when the compose
/// declares no `env_file:` (stacker's own generator writes values into
/// `environment:` instead, so this only arises with a hand-written
/// `deploy.compose_file`).
///
/// The author's fix is to move those values into an `environment:` block, where
/// they become literals in the image and survive.
pub fn env_file_values_lost_on_clone(
    compose_content: &str,
    env_values: &std::collections::BTreeMap<String, String>,
    protected: &BTreeSet<String>,
) -> Vec<String> {
    let declares_env_file = compose_content
        .lines()
        .any(|line| line.trim_start().starts_with("env_file:"));

    if !declares_env_file {
        return Vec::new();
    }

    env_values
        .keys()
        .filter(|key| !protected.contains(*key))
        .cloned()
        .collect()
}

/// Refuse a bake that has no field policy to sanitize against.
///
/// With no protected keys the embedded-secret scan substitutes nothing and the
/// `.env` scrub clears nothing, yet the bake would still print a confident
/// "Sanitized" line and publish an image carrying the author's credentials.
/// The usual causes are mundane — the template is not approved yet, so
/// `get_approved_by_slug` returns nothing; `DATABASE_URL` is unset so the
/// contract was never looked up; or the author declared no fields at all.
///
/// A stack that genuinely has no secrets can pass `--allow-unsanitized-snapshot`.
pub fn check_contract_usable(
    protected: &BTreeSet<String>,
    allow_unsanitized: bool,
) -> Result<(), crate::helpers::bake::BakeError> {
    if allow_unsanitized || !protected.is_empty() {
        return Ok(());
    }

    Err(crate::helpers::bake::BakeError::Finalize(
        "no config_contract fields resolved for this stack, so there is nothing to \
         sanitize and the image would keep the author's values. Usual causes: the \
         template is not approved yet, DATABASE_URL is not set, or the contract \
         declares no generated/provided fields. Pass --allow-unsanitized-snapshot \
         if the stack really has no secrets."
            .to_string(),
    ))
}

/// Report a blind spot in the embedded-secret scan, if there is one.
///
/// The scan finds a credential hidden inside a larger value (the DSN case) by
/// searching for the *values* the contract protects. With no values to search
/// for — no `.env` beside the compose — it cannot run, and a DSN would sail
/// through untouched.
///
/// A project having no `.env` is perfectly normal, so this is not an error: a
/// reference the buyer's machine cannot fill is already caught separately, by
/// [`crate::cli::generator::compose::resolve_non_contract_references`]. This
/// only says out loud that one check did not happen, because a mistyped
/// `--project-dir` looks identical from here.
pub fn env_scan_warning(
    env_values: &std::collections::BTreeMap<String, String>,
    protected: &BTreeSet<String>,
) -> Option<String> {
    if protected.is_empty() || !env_values.is_empty() {
        return None;
    }

    Some(
        "no values were found beside the compose file, so secrets embedded inside \
         larger values (a password inside DATABASE_URL, for example) could not be \
         searched for. If this stack does ship a .env, check --project-dir."
            .to_string(),
    )
}

/// Shell to strip everything host- or author-specific, so a clone boots as a
/// distinct machine that its author cannot reach.
///
/// Two separate problems, both solved by removal:
///
/// **Machine identity.** `sshd` regenerates host keys on first boot when none
/// are present and `systemd` repopulates an empty `/etc/machine-id`; clearing
/// cloud-init's instance state makes it treat the clone as a new instance and
/// re-run its per-instance modules. Left in place, every clone of the image
/// shares one host key, so any buyer can impersonate another buyer's server.
///
/// **The author's own access.** Hetzner's cloud-init *appends* the buyer's key
/// to `authorized_keys` — it never truncates the file. A key left in the image
/// therefore grants its holder root on every server ever cloned from that
/// snapshot. The same applies to private keys, `known_hosts`, and to
/// `~/.docker/config.json`, which holds base64 registry credentials whenever the
/// build performed a `docker login`.
///
/// `/etc/stacker/env` is deliberately absent from this list: cloud-init
/// overwrites it wholesale on the buyer's first boot (see `helpers::cloud_init`),
/// and the co-located `.env` is cleared separately by [`scrub_env_file`].
pub fn identity_reset_commands() -> Vec<String> {
    vec![
        // Machine identity.
        "rm -f /etc/ssh/ssh_host_*".to_string(),
        ": > /etc/machine-id".to_string(),
        "rm -f /var/lib/dbus/machine-id".to_string(),
        "rm -rf /var/lib/cloud/instances /var/lib/cloud/instance".to_string(),
        // The author's access — root and any other account on the box.
        "rm -f /root/.ssh/authorized_keys /home/*/.ssh/authorized_keys".to_string(),
        "rm -f /root/.ssh/id_* /home/*/.ssh/id_*".to_string(),
        "rm -f /root/.ssh/known_hosts /home/*/.ssh/known_hosts".to_string(),
        // Registry credentials left by a `docker login` during the build.
        "rm -f /root/.docker/config.json /home/*/.docker/config.json".to_string(),
        // Traces of the build itself.
        "rm -f /root/.bash_history /home/*/.bash_history".to_string(),
        "find /var/log -type f -exec truncate -s 0 {} + 2>/dev/null || true".to_string(),
    ]
}

/// Shell to stop the stack and drop its credential-bearing data volumes, so the
/// buyer's box initializes them from scratch with the buyer's own values.
///
/// Scoped to **this project's declared volumes only**. A build box also runs
/// platform-managed services in their own Compose projects — the nginx-proxy-manager
/// ingress and the status-panel agent — whose volumes hold Let's Encrypt
/// certificates and agent state. Enumerating the host (`docker volume ls` with no
/// filter) would delete those, and would additionally abort the bake: `docker volume
/// rm` refuses a volume still held by a running container, and `-f` only suppresses
/// "no such volume", not "volume is in use".
///
/// So the list comes from `docker compose config --volumes` (the names this stack
/// declares), and each is resolved to its real volume through Compose's own
/// `com.docker.compose.volume` label rather than by guessing the project prefix.
pub fn volume_reset_commands(
    project_dir: &str,
    keep: &[&str],
) -> Result<Vec<String>, crate::helpers::bake::BakeError> {
    if let Some(bad) = keep.iter().find(|name| !is_plain_volume_name(name)) {
        return Err(crate::helpers::bake::BakeError::Finalize(format!(
            "volume keep entry `{bad}` contains characters that are shell pattern syntax. \
             It would match something other than intended, and keeping a volume that should \
             have been reset leaves the author's credentials in the image. Use only letters, \
             digits, `_`, `-` and `.`."
        )));
    }

    // Match whole `_`/`-` separated segments rather than a bare substring, so
    // keeping `ollama` keeps `stackpilot_ollama` without also keeping
    // `not-ollama-backup`.
    let skip_kept = if keep.is_empty() {
        String::new()
    } else {
        let patterns = keep
            .iter()
            .flat_map(|name| {
                [
                    name.to_string(),
                    format!("*[-_]{name}"),
                    format!("{name}[-_]*"),
                    format!("*[-_]{name}[-_]*"),
                ]
            })
            .collect::<Vec<_>>()
            .join("|");
        format!("case \"$v\" in {patterns}) continue;; esac; ")
    };

    Ok(vec![
        format!("cd {project_dir} && docker compose down --remove-orphans"),
        format!(
            "cd {project_dir} && for v in $(docker compose config --volumes); do \
             {skip_kept}docker volume ls -q \
             --filter label=com.docker.compose.volume=\"$v\" \
             | xargs -r docker volume rm -f; done"
        ),
    ])
}

/// A volume name safe to interpolate into a shell `case` pattern: no `*`, `?`,
/// `[`, `|`, `)` or anything else the shell would read as syntax.
fn is_plain_volume_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Blank the author's secret values in a `.env` file while keeping the file's
/// shape: keys, comments, blank lines and non-secret values survive.
///
/// **The contract is the only authority.** A value is blanked when its key is
/// declared `generated`/`provided` in `config_contract`, or when the value
/// *contains* such a secret — the DSN case, where the credential hides inside a
/// larger string under a name no policy mentions
/// (`DATABASE_URL=postgres://user:<password>@host/db`). Nothing is guessed from
/// key names: name heuristics both miss real secrets and blank harmless values,
/// and the author already told us which fields are sensitive.
///
/// The keys are kept (rather than the lines dropped) so the file still
/// documents what the stack expects; on the buyer's box the whole file is
/// replaced from `/etc/stacker/env` by the systemd unit's `ExecStartPre`, so
/// the blanked values are never read.
pub fn scrub_env_file(content: &str, protected: &BTreeSet<String>) -> String {
    // The literal values the contract protects — what a DSN may be hiding.
    let secrets: Vec<String> = parse_env_pairs(content)
        .into_iter()
        .filter(|(key, value)| protected.contains(key) && value.len() >= MIN_EMBEDDED_SECRET_LEN)
        .map(|(_, value)| value)
        .collect();

    let mut out = String::with_capacity(content.len());

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        match line.split_once('=') {
            Some((key, value))
                if protected.contains(key.trim())
                    || secrets.iter().any(|secret| value.contains(secret.as_str())) =>
            {
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

/// Shortest value treated as a secret when searching *inside* another value.
/// Short strings collide with ordinary text and would blank harmless lines.
const MIN_EMBEDDED_SECRET_LEN: usize = 12;

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
    /// The author's field policy, parsed. Kept whole rather than flattened: the
    /// volume check needs to know *which service* declares a protected field,
    /// and flattening loses exactly that.
    pub contract: crate::cli::config_parser::ConfigContract,
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
                // Never echo the command back whole: a file write carries the
                // file's own contents as a base64 argument.
                return Err(format!("`{}` exited {code}: {stderr}", summarize(&cmd)));
            }
            Ok::<String, String>(stdout)
        }
    };

    // The flat set is still what the scrub and the parameterizer want.
    let protected_keys = protected_keys_from_contract(
        &serde_json::to_value(&ctx.contract).unwrap_or(serde_json::Value::Null),
    );

    let compose_path = format!("{}/docker-compose.yml", ctx.project_dir);
    let env_path = format!("{}/.env", ctx.project_dir);

    // The steps are not transactional, so a failure has to be able to say what
    // already happened — see `recovery_advice`.
    let mut done: Vec<FinalizeStage> = Vec::new();

    // Refuse a declaration that would ship the author's credentials, before
    // anything on the box is touched.
    if let Err(err) = check_volume_declarations(&ctx.contract) {
        disconnect_ssh(session).await;
        return Err(err);
    }

    let result = async {
        // 1. Read what the deploy left on the box.
        let compose = run(format!("cat {compose_path}"))
            .await
            .map_err(|e| fail("read compose", e))?;
        // A stack may legitimately have no .env; treat that as empty.
        let env_raw = run(format!("cat {env_path} 2>/dev/null || true"))
            .await
            .map_err(|e| fail("read .env", e))?;
        done.push(FinalizeStage::Read);
        let env_values = parse_env_pairs(&env_raw);

        let lost = env_file_values_lost_on_clone(&compose, &env_values, &protected_keys);
        if !lost.is_empty() {
            return Err(BakeError::Finalize(format!(
                "this compose reads values through `env_file:`, and {} of them are not \
                 contract fields ({}). The buyer's machine replaces that file wholesale \
                 from /etc/stacker/env, which carries only contract fields and the buyer's \
                 own values, so those would silently disappear at first boot. Move them \
                 into an `environment:` block, where they become part of the image.",
                lost.len(),
                lost.join(", ")
            )));
        }
        if let Some(warning) = env_scan_warning(&env_values, &protected_keys) {
            eprintln!("WARNING: {warning}");
        }

        // 2. Stop the stack and drop its credential-bearing data volumes, while
        //    the compose and .env on disk are still the ones Compose itself
        //    wrote. Doing this after the rewrites would hand `docker compose`
        //    a cleared .env, and any `${VAR}` outside an environment block
        //    (`image: ${REGISTRY}/app:${TAG}`) would then resolve empty and fail
        //    the teardown — with the files already modified and no way back.
        let keep = volumes_to_keep(&ctx.contract);
        let keep_refs: Vec<&str> = keep.iter().map(String::as_str).collect();
        for cmd in volume_reset_commands(&ctx.project_dir, &keep_refs)? {
            run(cmd).await.map_err(|e| fail("volume reset", e))?;
        }
        done.push(FinalizeStage::Teardown);

        // 3. Replace secrets embedded inside larger values (DSNs) with ${KEY}
        //    references. Whole-value keys were already parameterized at deploy
        //    time by `parameterize_compose_env_vars`.
        let sanitized = crate::cli::generator::compose::parameterize_embedded_secret_values(
            &compose,
            &env_values,
            &protected_keys,
        )
        .map_err(|conflict| BakeError::Finalize(conflict.to_string()))?;

        // 4. Put back the literal value of every reference the contract does not
        //    cover. Stage 4 in the CLI turns plain top-level `env:` keys into
        //    references too, and nothing fills those on the buyer's machine —
        //    the .env there is replaced wholesale from /etc/stacker/env, which
        //    only ever holds contract fields and the buyer's own values.
        let (sanitized, unresolved) =
            crate::cli::generator::compose::resolve_non_contract_references(
                &sanitized,
                &env_values,
                &protected_keys,
            );
        if !unresolved.is_empty() {
            let names: Vec<&str> = unresolved.iter().map(|r| r.name.as_str()).collect();
            return Err(BakeError::Finalize(format!(
                "the compose references {} environment variable(s) with no value on the \
                 build box and no default ({}). They are not contract fields, so nothing \
                 will fill them on a buyer's machine either — they would resolve to empty \
                 strings at boot.",
                names.len(),
                names.join(", ")
            )));
        }

        if sanitized != compose {
            write_remote_file(&run, &compose_path, &sanitized)
                .await
                .map_err(|e| fail("write compose", e))?;
            done.push(FinalizeStage::RewriteCompose);
        }

        // 5. Record what the image needs from the buyer's env file — taken from
        //    the contract, not from the text of the file. A reference only counts
        //    as required when something is expected to supply it.
        let required_env_keys =
            crate::cli::generator::compose::required_env_keys(&sanitized, &protected_keys);

        // 6. Clear the author's secrets in the co-located .env. The buyer's box
        //    overwrites this file wholesale from /etc/stacker/env at boot, so
        //    the blanked values are never read.
        if !env_raw.trim().is_empty() {
            let scrubbed = scrub_env_file(&env_raw, &protected_keys);
            write_remote_file(&run, &env_path, &scrubbed)
                .await
                .map_err(|e| fail("write .env", e))?;
            done.push(FinalizeStage::ClearEnv);
        }

        // 7. Strip machine identity last.
        for cmd in identity_reset_commands() {
            run(cmd).await.map_err(|e| fail("identity reset", e))?;
        }
        done.push(FinalizeStage::StripIdentity);

        Ok(FinalizeOutcome { required_env_keys })
    }
    .await;

    disconnect_ssh(session).await;

    result.map_err(|err| match err {
        BakeError::Finalize(message) => {
            BakeError::Finalize(format!("{message}\n\n{}", recovery_advice(&done)))
        }
        other => other,
    })
}

/// A step of [`finalize_build_box`], in the order they run.
///
/// Tracked so a failure can say what already happened: the steps are not
/// transactional and most of them are not reversible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeStage {
    /// Read the compose file and the co-located `.env`.
    Read,
    /// Stop the stack and drop its data volumes.
    Teardown,
    /// Write the sanitized compose back.
    RewriteCompose,
    /// Write the cleared `.env` back.
    ClearEnv,
    /// Remove machine identity and the author's access.
    StripIdentity,
}

impl FinalizeStage {
    fn describe(self) -> &'static str {
        match self {
            FinalizeStage::Read => "read the compose file and .env",
            FinalizeStage::Teardown => "stopped the stack and dropped its data volumes",
            FinalizeStage::RewriteCompose => "rewrote the compose file",
            FinalizeStage::ClearEnv => "cleared the .env",
            FinalizeStage::StripIdentity => "stripped machine identity and the author's access",
        }
    }
}

/// What the operator needs to know after a failed finalize.
///
/// Re-running against the same box is rarely equivalent: the compose is already
/// sanitized so the embedded-secret scan has nothing left to find, the `.env` is
/// already cleared so there are no values to search for, and the data volumes
/// are gone. Saying this plainly is the difference between a lost afternoon and
/// a fresh build box.
pub fn recovery_advice(completed: &[FinalizeStage]) -> String {
    let mut lines = Vec::new();

    if completed.is_empty() {
        lines.push("Nothing was changed on the build box.".to_string());
    } else {
        lines.push("Completed before the failure:".to_string());
        for stage in completed {
            lines.push(format!("  - {}", stage.describe()));
        }
    }

    let touched_files = completed
        .iter()
        .any(|s| matches!(s, FinalizeStage::RewriteCompose | FinalizeStage::ClearEnv));
    let torn_down = completed.contains(&FinalizeStage::Teardown);
    let lost_access = completed.contains(&FinalizeStage::StripIdentity);

    if lost_access {
        lines.push(
            "The box no longer accepts the bake key, so it cannot be reconnected to.".to_string(),
        );
    }
    if torn_down {
        lines.push(
            "Its data volumes are gone and the stack is stopped, so it no longer \
             represents a working deployment."
                .to_string(),
        );
    }
    if touched_files {
        lines.push(
            "Its compose and .env are already sanitized, so a retry would find nothing \
             left to search for and would report success over an unchecked image."
                .to_string(),
        );
    }

    if lost_access || torn_down || touched_files {
        lines.push("Deploy a fresh build box and bake that instead.".to_string());
    } else {
        lines.push("The bake can be retried against this box as-is.".to_string());
    }

    lines.join("\n")
}

/// A command shortened for an error message. A file write carries the file's
/// own contents as a base64 argument, which must not end up in the bake log.
fn summarize(cmd: &str) -> String {
    const MAX: usize = 120;
    if cmd.len() <= MAX {
        return cmd.to_string();
    }
    let head: String = cmd.chars().take(MAX).collect();
    format!("{head}… ({} chars)", cmd.len())
}

/// Source bytes per chunk. Kept a multiple of 3 so every chunk is a whole
/// number of base64 groups and decodes on its own; 48 KiB of source becomes
/// 64 KiB of base64, comfortably inside Linux's 128 KiB limit on a single
/// command-line argument (`MAX_ARG_STRLEN`).
const WRITE_CHUNK_BYTES: usize = 48 * 1024;

/// The shell to write `content` to `path`, split so no single command exceeds
/// the argument-length limit.
///
/// The payload travels base64-encoded, which sidesteps every quoting hazard —
/// a compose file is full of `$`, quotes and newlines. The first command
/// truncates, the rest append.
pub fn write_file_commands(path: &str, content: &str) -> Vec<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let bytes = content.as_bytes();
    if bytes.is_empty() {
        // Still create (or truncate) the file.
        return vec![format!(": > {path}")];
    }

    bytes
        .chunks(WRITE_CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| {
            let redirect = if index == 0 { ">" } else { ">>" };
            let encoded = STANDARD.encode(chunk);
            format!("printf %s {encoded} | base64 -d {redirect} {path}")
        })
        .collect()
}

/// Write `content` to `path` on the remote box.
async fn write_remote_file<F, Fut>(run: &F, path: &str, content: &str) -> Result<(), String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    for cmd in write_file_commands(path, content) {
        run(cmd).await?;
    }
    Ok(())
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

    fn contract(yaml: serde_json::Value) -> crate::cli::config_parser::ConfigContract {
        serde_json::from_value(yaml).expect("contract parses")
    }

    /// The author declares which volumes survive; the platform stops hardcoding
    /// a list per stack slug.
    #[test]
    fn kept_volumes_come_from_the_contract() {
        let c = contract(serde_json::json!({
            "services": {
                "ollama": { "volumes": { "app_ollama": { "mutability": "fixed" } } },
                "db": { "volumes": { "app_pgdata": { "mutability": "generated" } } }
            }
        }));
        let mut kept = volumes_to_keep(&c);
        kept.sort();
        assert_eq!(kept, vec!["app_ollama".to_string()]);
    }

    #[test]
    fn a_contract_declaring_no_volume_keeps_nothing() {
        let c = contract(serde_json::json!({
            "services": { "app": { "fields": { "SECRET_KEY": { "mutability": "generated", "type": "hex" } } } }
        }));
        assert!(volumes_to_keep(&c).is_empty());
    }

    /// The author decides which volumes are safe to keep, including on services
    /// that regenerate secrets — because a machine cannot tell the difference.
    ///
    /// Measured on real containers: a Postgres data directory stores
    /// `SCRAM-SHA-256$4096:...`, so the password is not in the volume in any
    /// searchable form; n8n keeps its own encryption key inside
    /// `database.sqlite`; a Qdrant volume holds only collections, because Qdrant
    /// reads its API key from the environment at every start. All three look
    /// identical to any automated check — yet the first two must be reset and the
    /// third must be kept. The difference is behavioural, not observable.
    ///
    /// An earlier revision refused a kept volume whenever its service declared a
    /// protected field. That rule would have forced `ai-knowledge-base` to
    /// recompute its embeddings on every buyer's machine — precisely the expense
    /// the snapshot exists to avoid.
    #[test]
    fn a_volume_of_a_service_with_generated_fields_is_the_authors_call() {
        let c = contract(serde_json::json!({
            "services": {
                "qdrant": {
                    "fields": { "QDRANT__SERVICE__API_KEY": { "mutability": "generated", "type": "alphanumeric" } },
                    "volumes": { "kb_qdrant_data": { "mutability": "fixed" } }
                }
            }
        }));
        assert!(
            check_volume_declarations(&c).is_ok(),
            "Qdrant reads its key from the environment; its volume holds only vectors"
        );
        assert_eq!(volumes_to_keep(&c), vec!["kb_qdrant_data".to_string()]);
    }

    #[test]
    fn a_service_without_protected_fields_may_keep_its_volume() {
        let c = contract(serde_json::json!({
            "services": {
                "ollama": { "volumes": { "app_ollama": { "mutability": "fixed" } } },
                "web": { "fields": { "LOG_LEVEL": { "mutability": "editable" } } }
            }
        }));
        assert!(check_volume_declarations(&c).is_ok());
    }

    /// Names now arrive from an author rather than a constant, so the shell
    /// pattern gate applies to them.
    #[test]
    fn an_author_supplied_name_with_shell_syntax_is_refused() {
        let c = contract(serde_json::json!({
            "services": { "app": { "volumes": { "oll*ama": { "mutability": "fixed" } } } }
        }));
        assert!(check_volume_declarations(&c).is_err());
    }

    /// L1 — a keep entry is interpolated into a shell `case` pattern, where
    /// `|`, `)`, `*`, `?` and `[` are all syntax. An entry carrying one of them
    /// would silently match something else, or break the command outright.
    /// Keeping a volume that should have been reset leaks the author's
    /// credentials, so this fails rather than guesses.
    #[test]
    fn a_keep_entry_with_shell_syntax_is_refused() {
        for bad in ["oll*ama", "a|b", "x)y", "a[bc]", "back`tick`", "semi;colon"] {
            assert!(
                volume_reset_commands("/home/trydirect/project", &[bad]).is_err(),
                "`{bad}` must not reach a shell pattern"
            );
        }
    }

    #[test]
    fn ordinary_volume_names_are_accepted() {
        for good in ["ollama", "stackpilot_ollama", "model-cache", "data1"] {
            assert!(
                volume_reset_commands("/home/trydirect/project", &[good]).is_ok(),
                "`{good}` should be usable"
            );
        }
    }

    /// Matching is on whole path segments, so `ollama` keeps
    /// `stackpilot_ollama` without also keeping `not-ollama-backup`.
    #[test]
    fn a_keep_entry_does_not_match_a_longer_unrelated_name() {
        let cmds = volume_reset_commands("/home/trydirect/project", &["ollama"])
            .expect("valid")
            .join(" ; ");
        assert!(
            !cmds.contains("*ollama*"),
            "a bare substring match would also keep `not-ollama-backup`: {cmds}"
        );
    }

    /// H3 — a service reading its values through `env_file:` takes them from the
    /// file, not through `${VAR}`. On the buyer's machine that file is replaced
    /// wholesale from `/etc/stacker/env`, which only carries contract fields and
    /// the buyer's own values, so everything else in it simply disappears.
    #[test]
    fn env_file_values_outside_the_contract_are_reported_as_lost() {
        let compose = "services:\n  app:\n    env_file:\n      - .env\n";
        let env: std::collections::BTreeMap<String, String> = [
            ("SECRET_KEY".to_string(), "value".to_string()),
            ("OLLAMA_MODEL".to_string(), "llama3.1".to_string()),
        ]
        .into_iter()
        .collect();

        let lost =
            env_file_values_lost_on_clone(compose, &env, &protected(["SECRET_KEY"].as_slice()));

        assert_eq!(
            lost,
            vec!["OLLAMA_MODEL".to_string()],
            "contract fields survive; anything else does not"
        );
    }

    #[test]
    fn a_compose_without_env_file_loses_nothing() {
        let compose = "services:\n  app:\n    environment:\n      A: b\n";
        let env: std::collections::BTreeMap<String, String> =
            [("OLLAMA_MODEL".to_string(), "llama3.1".to_string())]
                .into_iter()
                .collect();

        assert!(env_file_values_lost_on_clone(compose, &env, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn env_file_carrying_only_contract_fields_is_fine() {
        let compose = "services:\n  app:\n    env_file: .env\n";
        let env: std::collections::BTreeMap<String, String> =
            [("SECRET_KEY".to_string(), "value".to_string())]
                .into_iter()
                .collect();

        assert!(env_file_values_lost_on_clone(
            compose,
            &env,
            &protected(["SECRET_KEY"].as_slice())
        )
        .is_empty());
    }

    /// H5 — the steps are not transactional. When one fails, the operator has
    /// to be told what already happened, because most of it is not reversible
    /// and a retry against the same box is not equivalent.
    #[test]
    fn nothing_done_means_the_bake_can_be_retried() {
        let advice = recovery_advice(&[FinalizeStage::Read]);
        assert!(
            advice.contains("can be retried"),
            "a read-only failure leaves the box usable: {advice}"
        );
        assert!(!advice.contains("fresh build box"), "no need: {advice}");
    }

    #[test]
    fn modified_files_require_a_fresh_build_box() {
        let advice = recovery_advice(&[FinalizeStage::Read, FinalizeStage::RewriteCompose]);
        assert!(advice.contains("fresh build box"), "{advice}");
        assert!(
            advice.contains("already sanitized") || advice.contains("already-sanitized"),
            "the reason a retry is not equivalent should be stated: {advice}"
        );
    }

    #[test]
    fn a_completed_teardown_is_called_out_as_destructive() {
        let advice = recovery_advice(&[FinalizeStage::Read, FinalizeStage::Teardown]);
        assert!(advice.contains("data volumes"), "{advice}");
        assert!(advice.contains("fresh build box"), "{advice}");
    }

    #[test]
    fn a_completed_identity_reset_means_no_way_back_in() {
        let advice = recovery_advice(&[
            FinalizeStage::Read,
            FinalizeStage::Teardown,
            FinalizeStage::StripIdentity,
        ]);
        assert!(
            advice.contains("no longer accepts"),
            "losing SSH access must be stated plainly: {advice}"
        );
    }

    #[test]
    fn the_advice_lists_what_completed() {
        let advice = recovery_advice(&[FinalizeStage::Read, FinalizeStage::Teardown]);
        assert!(advice.contains("read"), "{advice}");
        assert!(advice.contains("stopped the stack"), "{advice}");
    }

    /// M4 — the payload travels as a shell command, and Linux caps a single
    /// argument at 128 KiB. A large compose must therefore be written in pieces
    /// rather than aborting the sanitize half-way with "Argument list too long".
    #[test]
    fn a_small_file_is_written_in_one_command() {
        let cmds = write_file_commands("/tmp/x", "hello");
        assert_eq!(cmds.len(), 1);
        assert!(
            cmds[0].contains("> /tmp/x"),
            "truncating write: {}",
            cmds[0]
        );
        assert!(!cmds[0].contains(">> /tmp/x"), "not appending: {}", cmds[0]);
    }

    #[test]
    fn a_large_file_is_written_in_appended_chunks() {
        let big = "x".repeat(300_000);
        let cmds = write_file_commands("/tmp/x", &big);

        assert!(cmds.len() > 1, "expected chunking, got {}", cmds.len());
        assert!(cmds[0].contains("> /tmp/x") && !cmds[0].contains(">> /tmp/x"));
        for cmd in &cmds[1..] {
            assert!(cmd.contains(">> /tmp/x"), "chunk must append: {cmd}");
        }
    }

    #[test]
    fn every_chunk_stays_under_the_argument_limit() {
        let big = "y".repeat(500_000);
        for cmd in write_file_commands("/tmp/x", &big) {
            assert!(
                cmd.len() < 128 * 1024,
                "a single command must fit in one argument, got {}",
                cmd.len()
            );
        }
    }

    /// Each chunk has to decode on its own, so the split must fall on a 3-byte
    /// boundary — otherwise base64 padding corrupts the seams.
    #[test]
    fn chunks_round_trip_to_the_original_content() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let original: String = (0..200_000)
            .map(|i| ((i % 26) as u8 + b'a') as char)
            .collect();
        let mut rebuilt = Vec::new();

        for cmd in write_file_commands("/tmp/x", &original) {
            let encoded = cmd.split_whitespace().nth(2).expect("printf %s <payload>");
            rebuilt.extend(STANDARD.decode(encoded).expect("each chunk decodes alone"));
        }

        assert_eq!(String::from_utf8(rebuilt).unwrap(), original);
    }

    #[test]
    fn an_empty_file_still_produces_a_write() {
        let cmds = write_file_commands("/tmp/x", "");
        assert_eq!(cmds.len(), 1, "the file must still be created/truncated");
    }

    /// H6 — a contract that did not resolve must stop the bake, not produce a
    /// confident "Sanitized" line over an image that still carries the author's
    /// credentials.
    #[test]
    fn empty_contract_refuses_the_bake() {
        let err = check_contract_usable(&BTreeSet::new(), false)
            .expect_err("an empty contract cannot sanitize anything");
        let message = err.to_string();
        assert!(
            message.contains("approved"),
            "the message should name the usual cause: {message}"
        );
    }

    #[test]
    fn empty_contract_is_allowed_only_deliberately() {
        assert!(check_contract_usable(&BTreeSet::new(), true).is_ok());
    }

    #[test]
    fn a_contract_with_fields_passes() {
        assert!(check_contract_usable(&protected(["SECRET_KEY"].as_slice()), false).is_ok());
    }

    /// M2 — a project may legitimately ship no `.env`, so its absence is not an
    /// error. What it does mean is that the embedded-secret scan has nothing to
    /// search for, which is a blind spot worth saying out loud — a wrong
    /// `--project-dir` looks exactly the same from here.
    #[test]
    fn missing_env_file_warns_when_secrets_are_declared() {
        let warning = env_scan_warning(
            &std::collections::BTreeMap::new(),
            &protected(["SECRET_KEY"].as_slice()),
        )
        .expect("a blind spot should be reported");
        assert!(
            warning.contains("--project-dir"),
            "the warning should name the likely cause: {warning}"
        );
    }

    #[test]
    fn missing_env_file_is_silent_when_nothing_is_declared() {
        assert!(env_scan_warning(&std::collections::BTreeMap::new(), &BTreeSet::new()).is_none());
    }

    #[test]
    fn present_env_values_warn_about_nothing() {
        let env: std::collections::BTreeMap<String, String> =
            [("SECRET_KEY".to_string(), "value".to_string())]
                .into_iter()
                .collect();
        assert!(env_scan_warning(&env, &protected(["SECRET_KEY"].as_slice())).is_none());
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

    /// Without a contract nothing is a declared secret, so nothing is blanked.
    /// The contract is the only authority — no name guessing.
    #[test]
    fn scrub_without_a_contract_blanks_nothing() {
        let env = "DB_PASSWORD=0123456789abcdef0123456789abcdef\nADMIN_USER=admin\n";
        let out = scrub_env_file(env, &BTreeSet::new());
        assert_eq!(out, env);
    }

    /// The DSN case: the credential hides inside a value whose own key is not
    /// declared anywhere.
    #[test]
    fn scrub_blanks_a_value_that_embeds_a_declared_secret() {
        let env = "POSTGRES_PASSWORD=0123456789abcdef0123456789abcdef\n\
                   DATABASE_URL=postgresql://stackpilot:0123456789abcdef0123456789abcdef@db/s\n\
                   REDIS_URL=redis://stackpilot-redis:6379\n";
        let out = scrub_env_file(env, &protected(["POSTGRES_PASSWORD"].as_slice()));

        assert!(out.contains("POSTGRES_PASSWORD=\n"), "declared key:\n{out}");
        assert!(out.contains("DATABASE_URL=\n"), "embedded secret:\n{out}");
        assert!(
            !out.contains("0123456789abcdef0123456789abcdef"),
            "no literal anywhere:\n{out}"
        );
        assert!(
            out.contains("REDIS_URL=redis://stackpilot-redis:6379"),
            "credential-free URL kept:\n{out}"
        );
    }

    /// A short declared value must not blank unrelated lines that happen to
    /// contain it as a substring.
    #[test]
    fn scrub_ignores_short_declared_values_when_scanning_inside_others() {
        let env = "PORT_TOKEN=8080\nPUBLIC_URL=http://host:8080/app\n";
        let out = scrub_env_file(env, &protected(["PORT_TOKEN"].as_slice()));
        assert!(
            out.contains("PORT_TOKEN=\n"),
            "declared key still blanked:\n{out}"
        );
        assert!(
            out.contains("PUBLIC_URL=http://host:8080/app"),
            "short value must not match inside others:\n{out}"
        );
    }

    #[test]
    fn scrub_preserves_comments_and_blank_lines() {
        let env = "# Secrets\n\nSECRET_KEY=abc\n";
        let out = scrub_env_file(env, &protected(["SECRET_KEY"].as_slice()));
        assert_eq!(out, "# Secrets\n\nSECRET_KEY=\n");
    }

    #[test]
    fn scrub_keeps_values_containing_equals_signs() {
        // A blanked key must not be confused by '=' inside a kept value.
        let env = "OLLAMA_MODEL=llama3.1\nJWT_SECRET=a=b=c\n";
        let out = scrub_env_file(env, &protected(["JWT_SECRET"].as_slice()));
        assert!(out.contains("OLLAMA_MODEL=llama3.1"), "kept:\n{out}");
        assert!(out.contains("JWT_SECRET=\n"), "blanked:\n{out}");
    }

    /// The author's own access must not survive into a buyer's server.
    ///
    /// Hetzner's cloud-init *appends* the buyer's key to `authorized_keys`; it
    /// never truncates the file. An author key left in the image therefore
    /// grants its holder root on every server ever cloned from that snapshot —
    /// the same class of defect as the shared host keys, and a worse one.
    #[test]
    fn identity_reset_removes_operator_access_to_the_image() {
        let cmds = identity_reset_commands().join(" ; ");
        assert!(
            cmds.contains("/root/.ssh/authorized_keys"),
            "the author's SSH access must not be baked in: {cmds}"
        );
        assert!(
            cmds.contains("/home/*/.ssh/authorized_keys"),
            "non-root accounts carry authorized_keys too: {cmds}"
        );
    }

    /// A registry login performed during the build leaves base64 credentials in
    /// `~/.docker/config.json`, which the snapshot would hand to every buyer.
    #[test]
    fn identity_reset_removes_registry_credentials() {
        let cmds = identity_reset_commands().join(" ; ");
        assert!(
            cmds.contains("/root/.docker/config.json"),
            "registry credentials must not be baked in: {cmds}"
        );
    }

    /// Private keys and the operator's known_hosts are equally author-specific.
    #[test]
    fn identity_reset_removes_operator_private_keys() {
        let cmds = identity_reset_commands().join(" ; ");
        assert!(
            cmds.contains("/root/.ssh/id_"),
            "operator private keys must not be baked in: {cmds}"
        );
        assert!(
            cmds.contains("known_hosts"),
            "known_hosts is author-specific: {cmds}"
        );
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
        let cmds = volume_reset_commands("/home/trydirect/project", &["ollama"])
            .expect("valid keep list")
            .join(" ; ");
        assert!(
            cmds.contains("docker compose down"),
            "stack stopped: {cmds}"
        );
        assert!(
            cmds.contains(r#"case "$v" in ollama|*[-_]ollama|"#),
            "kept volume skipped: {cmds}"
        );
    }

    /// Regression: enumerating the host would delete the nginx-proxy-manager
    /// ingress' certificates and the agent's state, and would abort the bake on
    /// the first volume still held by a running container.
    #[test]
    fn volume_reset_never_enumerates_the_whole_host() {
        for keep in [&[][..], &["ollama"][..]] {
            let cmds = volume_reset_commands("/home/trydirect/project", keep)
                .expect("valid keep list")
                .join(" ; ");
            assert!(
                !cmds.contains("docker volume ls -q |"),
                "must not pipe an unfiltered host-wide listing: {cmds}"
            );
            assert!(
                cmds.contains("docker compose config --volumes"),
                "volume list must come from the project's compose: {cmds}"
            );
            assert!(
                cmds.contains("--filter label=com.docker.compose.volume="),
                "removal must be scoped by compose's own label: {cmds}"
            );
        }
    }

    #[test]
    fn volume_reset_without_a_keep_list_skips_nothing() {
        let cmds = volume_reset_commands("/home/trydirect/project", &[])
            .expect("valid keep list")
            .join(" ; ");
        assert!(!cmds.contains("case "), "no skip clause: {cmds}");
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

    /// A contract that declares nothing keeps nothing — the same default the
    /// stack-slug `match` used to give an unlisted stack, now without needing
    /// the platform to know the stack at all.
    #[test]
    fn a_contract_that_declares_nothing_keeps_nothing() {
        let empty = crate::cli::config_parser::ConfigContract::default();
        assert!(volumes_to_keep(&empty).is_empty());
        assert!(check_volume_declarations(&empty).is_ok());
    }
}
