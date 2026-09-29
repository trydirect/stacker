//! Shared deployment context resolution.
//!
//! Single source of truth for "where does this stack live?" — consumed by
//! `logs`, `status`, `agent`, `proxy`, `pipe`, and friends.
//!
//! Resolution order (facts beat declarations):
//! 1. Explicit `--deployment <HASH>` flag → `Remote(hash)`.
//! 2. `.stacker/active-target` → that target's context (written by
//!    `stacker target` and by `stacker deploy` — unless the deployment watch
//!    reported failure).
//! 3. Deployment locks: exactly one → adopt it (and self-heal
//!    `active-target`); more than one → ambiguous, the user must pick with
//!    `stacker target <local|cloud|server>`.
//! 4. `stacker.yml` `deploy.target` as a last resort.
//!
//! A local deployment never needs a deployment hash: it is inspected through
//! Docker directly. Only `Remote` contexts resolve a hash via the Stacker API.
//! Agent-mediated commands use [`resolve_agent_deployment_hash`], which never
//! gates on a local placement — a pinned `deploy.deployment_hash` stays
//! reachable even when the active target is `local`.

use std::path::Path;

use crate::cli::config_parser::{DeployTarget, StackerConfig};
use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::error::CliError;
use crate::cli::runtime::CliRuntime;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Error message shown when multiple deployment locks exist and no
/// `.stacker/active-target` disambiguates them. Matches the wording of
/// `stacker target` with no argument.
pub const AMBIGUOUS_TARGET_MESSAGE: &str =
    "No active target set. Use: stacker target <local|cloud|server>";

/// Error message shown when no deployment evidence exists at all.
const UNKNOWN_CONTEXT_MESSAGE: &str = "Cannot determine deployment context.\n\
     Use --deployment <HASH>, run `stacker target local` for local mode,\n\
     or run from a directory with a deployment lock or stacker.yml.";

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Placement — API-free classification of where the stack lives
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Where the stack currently lives, derived from lockfiles and config only.
///
/// `Unknown` means no evidence exists at all (no active-target, no lock, no
/// usable `stacker.yml`) — callers typically fall through to their natural
/// "No deployment found. Run 'stacker deploy' first." error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentPlacement {
    /// Local Docker deployment (inspect via `docker compose`).
    Local,
    /// Remote deployment on target `cloud` or `server` (inspect via agent/API).
    Remote {
        /// The deploy target the remote stack lives on ("cloud" / "server").
        target: String,
    },
    /// No deployment evidence found.
    Unknown,
}

impl DeploymentPlacement {
    pub fn is_local(&self) -> bool {
        matches!(self, DeploymentPlacement::Local)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, DeploymentPlacement::Remote { .. })
    }
}

/// List the deployment targets that have a lockfile in `.stacker/`.
///
/// Counts per-target locks AND the legacy `deployment.lock` (deduped by
/// target) — a mixed legacy + per-target state is genuinely ambiguous and
/// must not be silently collapsed into one.
fn locked_targets(project_dir: &Path) -> Result<Vec<String>, CliError> {
    let mut found = Vec::new();
    for target in ["cloud", "server", "local"] {
        if DeploymentLock::exists_for_target(project_dir, target) {
            found.push(target.to_string());
        }
    }
    if let Some(legacy) = DeploymentLock::load_legacy(project_dir, None)? {
        if !found.contains(&legacy.target) {
            found.push(legacy.target.clone());
        }
    }
    Ok(found)
}

fn placement_for_target(target: &str) -> DeploymentPlacement {
    if target == "local" {
        DeploymentPlacement::Local
    } else {
        DeploymentPlacement::Remote {
            target: target.to_string(),
        }
    }
}

/// Classify where the stack lives without contacting the Stacker API.
///
/// When exactly one deployment lock exists and no active target is set, the
/// lock is adopted and `.stacker/active-target` is written (best-effort
/// self-heal) so subsequent resolutions are unambiguous.
pub fn resolve_deploy_placement(project_dir: &Path) -> Result<DeploymentPlacement, CliError> {
    // 1. Explicit active target (set by `stacker target` / `stacker deploy` / `stacker init`).
    if let Some(target) = DeploymentLock::read_active_target(project_dir)? {
        return match target.as_str() {
            "local" => Ok(DeploymentPlacement::Local),
            "cloud" | "server" => Ok(DeploymentPlacement::Remote { target }),
            other => Err(CliError::ConfigValidation(format!(
                "Unknown active target '{}'. Use: local, cloud, or server.",
                other
            ))),
        };
    }

    // 2. Deployment locks are the facts on the ground.
    let locked = locked_targets(project_dir)?;
    match locked.len() {
        1 => {
            let target = locked[0].clone();
            // Self-heal: record the disambiguator so this branch never fires twice.
            let _ = DeploymentLock::write_active_target(project_dir, &target);
            Ok(placement_for_target(&target))
        }
        0 => {
            // 3. No locks — fall back to the declared target in stacker.yml.
            let config_path = project_dir.join(DEFAULT_CONFIG_FILE);
            if let Ok(config) = StackerConfig::from_file(&config_path)
                .and_then(|config| config.with_resolved_deploy_target(None))
            {
                return Ok(match config.deploy.target {
                    DeployTarget::Local => DeploymentPlacement::Local,
                    other => placement_for_target(&other.to_string()),
                });
            }
            Ok(DeploymentPlacement::Unknown)
        }
        _ => Err(CliError::ConfigValidation(format!(
            "{}\n\
             Multiple deployment locks found: {}.",
            AMBIGUOUS_TARGET_MESSAGE,
            locked.join(", ")
        ))),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DeploymentContext — placement + resolved deployment hash
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Deployment context resolved from CLI flags, active target, or lockfiles.
#[derive(Debug, Clone, PartialEq)]
pub enum DeploymentContext {
    /// Remote deployment identified by hash.
    Remote(String),
    /// Local mode — no deployment hash, operations run against local Docker.
    Local,
}

impl DeploymentContext {
    /// Returns `true` when in local mode.
    pub fn is_local(&self) -> bool {
        matches!(self, DeploymentContext::Local)
    }

    /// Returns the deployment hash if remote.
    pub fn hash(&self) -> Option<&str> {
        match self {
            DeploymentContext::Remote(h) => Some(h),
            DeploymentContext::Local => None,
        }
    }
}

/// Helper that prepends `[local] ` when in local mode.
pub fn mode_prefix(ctx_mode: &DeploymentContext) -> &'static str {
    match ctx_mode {
        DeploymentContext::Local => "\x1b[36m[local]\x1b[0m ",
        DeploymentContext::Remote(_) => "",
    }
}

/// Error shown when a remote-only command runs against a local deployment.
fn local_target_error() -> CliError {
    CliError::ConfigValidation(
        "This command requires a remote deployment, but the active target is 'local'.\n\
         Use --deployment <HASH>, or switch with: stacker target <cloud|server>"
            .to_string(),
    )
}

/// Pure decision step before any API lookup: explicit flag or
/// placement-derived context.
#[derive(Debug, Clone, PartialEq)]
enum ContextDecision {
    /// Resolved without touching the API.
    Resolved(DeploymentContext),
    /// Remote placement — the hash must be resolved via the Stacker API.
    NeedsRemoteHash(String),
}

fn decide_context(
    explicit: &Option<String>,
    project_dir: &Path,
) -> Result<ContextDecision, CliError> {
    // Explicit flag always wins.
    if let Some(hash) = explicit {
        if !hash.is_empty() {
            return Ok(ContextDecision::Resolved(DeploymentContext::Remote(
                hash.clone(),
            )));
        }
    }

    match resolve_deploy_placement(project_dir)? {
        DeploymentPlacement::Local => Ok(ContextDecision::Resolved(DeploymentContext::Local)),
        DeploymentPlacement::Remote { target } => Ok(ContextDecision::NeedsRemoteHash(target)),
        DeploymentPlacement::Unknown => Err(CliError::ConfigValidation(
            UNKNOWN_CONTEXT_MESSAGE.to_string(),
        )),
    }
}

/// Resolve the deployment context from an explicit hash flag, the active
/// target, deployment locks, or `stacker.yml`.
///
/// Resolution order:
/// 1. Explicit `--deployment` flag value → `Remote(hash)`
/// 2. Placement (`resolve_deploy_placement`): `Local` → `Local`;
///    `Remote` → resolve the deployment hash via the Stacker API.
pub fn resolve_deployment_context(
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<DeploymentContext, CliError> {
    let project_dir = std::env::current_dir().map_err(CliError::Io)?;
    resolve_deployment_context_at(&project_dir, explicit, ctx)
}

/// [`resolve_deployment_context`] with an explicit project directory.
pub fn resolve_deployment_context_at(
    project_dir: &Path,
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<DeploymentContext, CliError> {
    match decide_context(explicit, project_dir)? {
        ContextDecision::Resolved(context) => Ok(context),
        ContextDecision::NeedsRemoteHash(target) => Ok(DeploymentContext::Remote(
            resolve_remote_hash(project_dir, Some(&target), ctx)?,
        )),
    }
}

/// Resolve a deployment hash, erroring when the active context is local.
///
/// Same as [`resolve_deployment_context`] for remote deployments; a local
/// deployment has no hash by design. Used by `pipe` commands that require a
/// remote deployment.
pub fn resolve_deployment_hash(
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    let project_dir = std::env::current_dir().map_err(CliError::Io)?;
    resolve_deployment_hash_at(&project_dir, explicit, ctx)
}

/// [`resolve_deployment_hash`] with an explicit project directory.
pub fn resolve_deployment_hash_at(
    project_dir: &Path,
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    match decide_context(explicit, project_dir)? {
        ContextDecision::Resolved(DeploymentContext::Remote(hash)) => Ok(hash),
        ContextDecision::Resolved(DeploymentContext::Local) => Err(local_target_error()),
        ContextDecision::NeedsRemoteHash(target) => {
            resolve_remote_hash(project_dir, Some(&target), ctx)
        }
    }
}

/// Resolve a deployment hash for agent-mediated commands (`stacker agent *`,
/// single-service deploy, `monitor`).
///
/// These always talk to a remote agent, so — unlike
/// [`resolve_deployment_hash`] — a local placement never short-circuits: the
/// pinned `deploy.deployment_hash` (written by `stacker agent install`) and
/// the API hash chain stay reachable even when the active target is `local`.
pub fn resolve_agent_deployment_hash(
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    let project_dir = std::env::current_dir().map_err(CliError::Io)?;
    resolve_agent_deployment_hash_at(&project_dir, explicit, ctx)
}

/// [`resolve_agent_deployment_hash`] with an explicit project directory.
pub fn resolve_agent_deployment_hash_at(
    project_dir: &Path,
    explicit: &Option<String>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    if let Some(hash) = explicit {
        if !hash.is_empty() {
            return Ok(hash.clone());
        }
    }
    resolve_remote_hash(project_dir, None, ctx)
}

/// Best-effort local context probe used by pipe commands: `Some(Local)` when
/// the resolved placement is local and no explicit hash was requested,
/// `None` otherwise. Never contacts the API.
pub fn resolve_local_deployment_context(
    explicit: &Option<String>,
    project_dir: &Path,
) -> Result<Option<DeploymentContext>, CliError> {
    if explicit
        .as_ref()
        .map(|hash| !hash.trim().is_empty())
        .unwrap_or(false)
    {
        return Ok(None);
    }

    match resolve_deploy_placement(project_dir)? {
        DeploymentPlacement::Local => Ok(Some(DeploymentContext::Local)),
        _ => Ok(None),
    }
}

/// Resolve the deployment hash for a remote deployment.
///
/// Chain:
/// 1. `stacker.yml` `deploy.deployment_hash` (pinned by agent install/deploy).
/// 2. Deployment lock → `deployment_id` → Stacker API lookup. When
///    `preferred_target` is set, that target's lock is used; otherwise any
///    lock (agent commands are target-agnostic).
/// 3. `stacker.yml` project identity/name → active agent (most recent heartbeat).
/// 4. `stacker.yml` project identity/name → most recent deployment.
pub(crate) fn resolve_remote_hash(
    project_dir: &Path,
    preferred_target: Option<&str>,
    ctx: &CliRuntime,
) -> Result<String, CliError> {
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);
    let config = if config_path.exists() {
        StackerConfig::from_file(&config_path)
            .and_then(|config| config.with_resolved_deploy_target(None))
            .ok()
    } else {
        None
    };

    // 1. Explicit deployment hash recorded in stacker.yml.
    if let Some(hash) = config
        .as_ref()
        .and_then(|c| c.deploy.deployment_hash.as_ref())
    {
        if !hash.trim().is_empty() {
            return Ok(hash.clone());
        }
    }

    // 2. Lock → integer deployment ID → API hash lookup.
    let lock = match preferred_target {
        Some(target) => DeploymentLock::load_for_target(project_dir, target)?,
        None => DeploymentLock::load(project_dir)?,
    };
    if let Some(dep_id) = lock.as_ref().and_then(|l| l.deployment_id) {
        if let Some(info) = ctx.block_on(ctx.client.get_deployment_status(dep_id as i32))? {
            return Ok(info.deployment_hash);
        }
    }

    // Project identity, falling back to config name.
    let project_name = config.as_ref().map(|config| {
        config
            .project
            .identity
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| config.name.clone())
    });
    let project_name = project_name.as_deref().map(str::trim).unwrap_or("");

    if !project_name.is_empty() {
        if let Ok(Some(proj)) = ctx.block_on(ctx.client.find_project_by_name(project_name)) {
            // 3. Active agent for the project (most reliable).
            match ctx.block_on(ctx.client.agent_snapshot_by_project(proj.id)) {
                Ok((_, hash)) => {
                    eprintln!(
                        "\x1b[2mℹ No --deployment specified — using active agent for project '{}': {}\x1b[0m",
                        project_name, hash
                    );
                    return Ok(hash);
                }
                Err(_) => {}
            }

            // 4. Most recent deployment for the project.
            if let Ok(deployments) =
                ctx.block_on(ctx.client.list_deployments(Some(proj.id), Some(1)))
            {
                if let Some(dep) = deployments.into_iter().next() {
                    return Ok(dep.deployment_hash);
                }
            }
        }
    }

    // Build the error from the evidence actually found.
    let mut evidence: Vec<String> = Vec::new();
    match lock.as_ref() {
        Some(lock) => match lock.deployment_id {
            Some(dep_id) => evidence.push(format!(
                "The deployment lock for target '{}' (deployment id {}) cannot be resolved on the active Stacker API.",
                lock.target, dep_id
            )),
            None => evidence.push(format!(
                "The deployment lock for target '{}' carries no deployment id.",
                lock.target
            )),
        },
        None => evidence.push("No deployment lock was found in .stacker/.".to_string()),
    }
    if project_name.is_empty() {
        evidence.push("No stacker.yml project is configured for API lookup.".to_string());
    } else {
        evidence.push(format!(
            "Project '{}' was not found on the active Stacker API (or has no agent or deployment).",
            project_name
        ));
    }

    Err(CliError::ConfigValidation(format!(
        "Cannot determine deployment hash{}.\n\
         {}\n\
         Use --deployment <HASH>, verify the API with `stacker whoami`, or switch the target \
with `stacker target <local|cloud|server>`.",
        preferred_target
            .map(|t| format!(" for the '{t}' deployment"))
            .unwrap_or_default(),
        evidence.join(" ")
    )))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config_parser::ServerConfig;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_config(dir: &Path, body: &str) {
        std::fs::write(dir.join(DEFAULT_CONFIG_FILE), body).unwrap();
    }

    fn write_local_lock(dir: &Path) {
        DeploymentLock::for_local().save(dir).unwrap();
    }

    fn write_server_lock(dir: &Path) {
        DeploymentLock::for_server(&ServerConfig {
            host: "203.0.113.10".to_string(),
            user: "root".to_string(),
            ssh_key: Some(PathBuf::from("/tmp/id_ed25519")),
            port: 22,
        })
        .save(dir)
        .unwrap();
    }

    // ── Placement: active-target wins ────────────────────────────────

    #[test]
    fn active_target_local_wins_over_server_lock() {
        // Regression: posthog repro — both locks present, local selected
        // explicitly via `stacker target local`.
        let dir = TempDir::new().unwrap();
        write_server_lock(dir.path());
        write_local_lock(dir.path());
        write_config(dir.path(), "name: posthog\ndeploy:\n  target: server\n");
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let placement = resolve_deploy_placement(dir.path()).unwrap();
        assert_eq!(placement, DeploymentPlacement::Local);
    }

    #[test]
    fn active_target_server_wins_over_newer_local_lock() {
        let dir = TempDir::new().unwrap();
        write_local_lock(dir.path());
        write_server_lock(dir.path());
        DeploymentLock::write_active_target(dir.path(), "server").unwrap();

        let placement = resolve_deploy_placement(dir.path()).unwrap();
        assert_eq!(
            placement,
            DeploymentPlacement::Remote {
                target: "server".to_string()
            }
        );
    }

    #[test]
    fn unknown_active_target_is_rejected() {
        let dir = TempDir::new().unwrap();
        DeploymentLock::write_active_target(dir.path(), "Local").unwrap();

        let err = resolve_deploy_placement(dir.path()).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("Unknown active target 'Local'"),
            "got: {}",
            msg
        );
    }

    // ── Placement: ambiguity ─────────────────────────────────────────

    #[test]
    fn multiple_locks_without_active_target_are_ambiguous() {
        // Regression: posthog repro — deployment-local.lock (new) and
        // deployment-server.lock (old), no .stacker/active-target. Must not
        // silently prefer one of them.
        let dir = TempDir::new().unwrap();
        write_server_lock(dir.path());
        write_local_lock(dir.path());
        write_config(dir.path(), "name: posthog\ndeploy:\n  target: server\n");

        let err = resolve_deploy_placement(dir.path()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains(AMBIGUOUS_TARGET_MESSAGE), "got: {}", msg);
        assert!(
            msg.contains("Multiple deployment locks found: server, local."),
            "got: {}",
            msg
        );
    }

    #[test]
    fn mixed_legacy_and_per_target_locks_are_ambiguous() {
        // Upgraded project: legacy deployment.lock (target cloud) plus a newer
        // deployment-local.lock. Two targets' worth of evidence = ambiguous.
        let dir = TempDir::new().unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        let mut legacy = DeploymentLock::for_local();
        legacy.target = "cloud".to_string();
        std::fs::write(
            stacker_dir.join("deployment.lock"),
            serde_yaml::to_string(&legacy).unwrap(),
        )
        .unwrap();
        write_local_lock(dir.path());

        let err = resolve_deploy_placement(dir.path()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains(AMBIGUOUS_TARGET_MESSAGE), "got: {}", msg);
    }

    // ── Placement: single lock + self-heal ───────────────────────────

    #[test]
    fn single_local_lock_adopts_and_self_heals_active_target() {
        let dir = TempDir::new().unwrap();
        write_local_lock(dir.path());
        write_config(dir.path(), "name: demo\ndeploy:\n  target: server\n");

        let placement = resolve_deploy_placement(dir.path()).unwrap();
        assert_eq!(placement, DeploymentPlacement::Local);
        assert_eq!(
            DeploymentLock::read_active_target(dir.path()).unwrap(),
            Some("local".to_string())
        );
    }

    #[test]
    fn single_server_lock_adopts_and_self_heals_active_target() {
        let dir = TempDir::new().unwrap();
        write_server_lock(dir.path());

        let placement = resolve_deploy_placement(dir.path()).unwrap();
        assert_eq!(
            placement,
            DeploymentPlacement::Remote {
                target: "server".to_string()
            }
        );
        assert_eq!(
            DeploymentLock::read_active_target(dir.path()).unwrap(),
            Some("server".to_string())
        );
    }

    #[test]
    fn legacy_single_lock_counts_as_one() {
        let dir = TempDir::new().unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        let lock = DeploymentLock::for_server(&ServerConfig {
            host: "203.0.113.11".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        });
        std::fs::write(
            stacker_dir.join("deployment.lock"),
            serde_yaml::to_string(&lock).unwrap(),
        )
        .unwrap();

        let placement = resolve_deploy_placement(dir.path()).unwrap();
        assert_eq!(
            placement,
            DeploymentPlacement::Remote {
                target: "server".to_string()
            }
        );
        assert_eq!(
            DeploymentLock::read_active_target(dir.path()).unwrap(),
            Some("server".to_string())
        );
    }

    #[test]
    fn legacy_lock_with_same_target_is_deduped() {
        let dir = TempDir::new().unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        let lock = DeploymentLock::for_server(&ServerConfig {
            host: "203.0.113.12".to_string(),
            user: "root".to_string(),
            ssh_key: None,
            port: 22,
        });
        std::fs::write(
            stacker_dir.join("deployment.lock"),
            serde_yaml::to_string(&lock).unwrap(),
        )
        .unwrap();
        write_server_lock(dir.path());

        assert_eq!(
            resolve_deploy_placement(dir.path()).unwrap(),
            DeploymentPlacement::Remote {
                target: "server".to_string()
            }
        );
    }

    // ── Placement: stacker.yml fallback ──────────────────────────────

    #[test]
    fn no_locks_fall_back_to_config_target_local() {
        let dir = TempDir::new().unwrap();
        write_config(dir.path(), "name: demo\ndeploy:\n  target: local\n");

        assert_eq!(
            resolve_deploy_placement(dir.path()).unwrap(),
            DeploymentPlacement::Local
        );
    }

    #[test]
    fn no_locks_fall_back_to_config_target_server() {
        let dir = TempDir::new().unwrap();
        write_config(
            dir.path(),
            "name: demo\ndeploy:\n  target: server\n  server:\n    host: 10.0.0.8\n",
        );

        assert_eq!(
            resolve_deploy_placement(dir.path()).unwrap(),
            DeploymentPlacement::Remote {
                target: "server".to_string()
            }
        );
    }

    #[test]
    fn no_evidence_is_unknown() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            resolve_deploy_placement(dir.path()).unwrap(),
            DeploymentPlacement::Unknown
        );
    }

    // ── Context decisions (API-free paths) ───────────────────────────

    #[test]
    fn explicit_flag_wins_over_local_placement() {
        let dir = TempDir::new().unwrap();
        write_local_lock(dir.path());
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let explicit = Some("deployment_abc".to_string());
        assert_eq!(
            decide_context(&explicit, dir.path()).unwrap(),
            ContextDecision::Resolved(DeploymentContext::Remote("deployment_abc".to_string()))
        );
    }

    #[test]
    fn local_placement_decides_local() {
        let dir = TempDir::new().unwrap();
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        assert_eq!(
            decide_context(&None, dir.path()).unwrap(),
            ContextDecision::Resolved(DeploymentContext::Local)
        );
    }

    #[test]
    fn unknown_placement_errors_with_actionable_message() {
        let dir = TempDir::new().unwrap();
        let err = decide_context(&None, dir.path()).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("Cannot determine deployment context"),
            "got: {}",
            msg
        );
    }

    #[test]
    fn hash_resolution_errors_on_local_placement() {
        let dir = TempDir::new().unwrap();
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let err =
            resolve_deployment_hash_at(dir.path(), &None, &CliRuntime::for_tests()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("active target is 'local'"), "got: {}", msg);
    }

    // ── Agent hash resolution (M1 regression) ────────────────────────

    #[test]
    fn agent_hash_reaches_pinned_hash_despite_local_placement() {
        // Regression: `stacker agent install` pins deploy.deployment_hash so
        // later `stacker agent status/logs` work without --deployment. A local
        // active target must not block that path.
        let dir = TempDir::new().unwrap();
        write_local_lock(dir.path());
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();
        write_config(
            dir.path(),
            "name: demo\ndeploy:\n  deployment_hash: deployment_abc\n",
        );

        let hash =
            resolve_agent_deployment_hash_at(dir.path(), &None, &CliRuntime::for_tests()).unwrap();
        assert_eq!(hash, "deployment_abc");
    }

    #[test]
    fn agent_hash_honest_error_without_lock() {
        let dir = TempDir::new().unwrap();
        let err = resolve_agent_deployment_hash_at(dir.path(), &None, &CliRuntime::for_tests())
            .unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("No deployment lock was found"), "got: {}", msg);
        assert!(msg.contains("stacker whoami"), "got: {}", msg);
        assert!(!msg.contains("is present, but"), "got: {}", msg);
    }

    // ── Local context probe ──────────────────────────────────────────

    #[test]
    fn local_context_probe_respects_explicit_hash() {
        let dir = TempDir::new().unwrap();
        DeploymentLock::write_active_target(dir.path(), "local").unwrap();

        let explicit = Some("deployment_abc".to_string());
        assert_eq!(
            resolve_local_deployment_context(&explicit, dir.path()).unwrap(),
            None
        );
        assert_eq!(
            resolve_local_deployment_context(&None, dir.path()).unwrap(),
            Some(DeploymentContext::Local)
        );
    }

    #[test]
    fn local_context_probe_none_for_remote_placement() {
        let dir = TempDir::new().unwrap();
        write_server_lock(dir.path());
        assert_eq!(
            resolve_local_deployment_context(&None, dir.path()).unwrap(),
            None
        );
    }

    // ── Context helpers ──────────────────────────────────────────────

    #[test]
    fn context_helpers() {
        assert!(DeploymentContext::Local.is_local());
        assert_eq!(DeploymentContext::Local.hash(), None);

        let remote = DeploymentContext::Remote("deployment_abc".to_string());
        assert!(!remote.is_local());
        assert_eq!(remote.hash(), Some("deployment_abc"));

        assert_eq!(
            mode_prefix(&DeploymentContext::Local),
            "\x1b[36m[local]\x1b[0m "
        );
        assert_eq!(mode_prefix(&remote), "");
    }
}
