//! `deploy_ephemeral` — orchestrate a throwaway, auto-expiring deployment.
//!
//! All the policy logic (TTL clamping, per-user quota, compose safety gate,
//! reaper expired-set selection) is pure and lives here; the actual
//! provision/status/teardown is injected via [`SandboxController`] so this
//! unit-tests without MQ, DB or cloud.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{AgentToolError, Result};

/// Operator-configured limits for the managed sandbox pool.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxQuota {
    pub default_ttl_secs: u64,
    pub max_ttl_secs: u64,
    pub max_concurrent_per_user: u32,
}

impl Default for SandboxQuota {
    fn default() -> Self {
        SandboxQuota {
            default_ttl_secs: 30 * 60, // 30m
            max_ttl_secs: 2 * 60 * 60, // 2h
            max_concurrent_per_user: 3,
        }
    }
}

/// A validated launch request handed to the controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxSpec {
    pub compose_yaml: String,
    pub ttl_secs: u64,
    /// Absolute expiry (epoch seconds) — the reaper tears down past this.
    pub expires_at: u64,
}

/// Reference to a launched sandbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxHandle {
    /// Deployment hash, e.g. "sandbox_<uuid>".
    pub id: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Provisioning,
    Healthy,
    Unhealthy,
    Expired,
}

/// Current state of a sandbox, reported back to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxStatus {
    pub id: String,
    pub health: Health,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_excerpt: Option<String>,
    pub expires_at: u64,
}

/// Provisions / inspects / destroys sandboxes. Real impl publishes to MQ and
/// polls the deployment DB; mocked in tests. v1 = FreshVmController, v2 = Kata pool.
#[async_trait]
pub trait SandboxController: Send + Sync {
    async fn launch(&self, spec: &SandboxSpec) -> Result<SandboxHandle>;
    async fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus>;
    async fn teardown(&self, handle: &SandboxHandle) -> Result<()>;
}

/// Clamp a requested TTL to `[1, max]`, defaulting when unset.
pub fn clamp_ttl(requested_secs: Option<u64>, quota: &SandboxQuota) -> u64 {
    match requested_secs {
        None => quota.default_ttl_secs,
        Some(r) => r.clamp(1, quota.max_ttl_secs),
    }
}

/// Deny when the user is already at their concurrent-sandbox limit.
pub fn quota_check(current_active: u32, quota: &SandboxQuota) -> Result<()> {
    if current_active >= quota.max_concurrent_per_user {
        return Err(AgentToolError::QuotaExceeded(format!(
            "already at {} concurrent sandboxes (max {})",
            current_active, quota.max_concurrent_per_user
        )));
    }
    Ok(())
}

/// Reject agent-supplied compose that could compromise the host, BEFORE any
/// provisioning: privileged services, the Docker socket, host networking.
pub fn gate_compose(compose_yaml: &str) -> Result<()> {
    // Must parse as compose (reuse td-audit's tolerant parser).
    let model = td_audit::compose::parse_compose(compose_yaml)
        .map_err(|e| AgentToolError::ComposeRejected(format!("not a valid compose file: {e}")))?;

    if let Some(svc) = model.services.iter().find(|s| s.privileged) {
        return Err(AgentToolError::ComposeRejected(format!(
            "service '{}' requests privileged mode",
            svc.name
        )));
    }

    // Raw-string checks for host-escape vectors the typed model doesn't capture.
    let lower = compose_yaml.to_lowercase();
    if lower.contains("docker.sock") {
        return Err(AgentToolError::ComposeRejected(
            "mounting the Docker socket is not allowed".into(),
        ));
    }
    if lower.contains("network_mode") && lower.contains("host") {
        return Err(AgentToolError::ComposeRejected(
            "host networking is not allowed".into(),
        ));
    }
    Ok(())
}

/// Which handles have passed their expiry as of `now` — the reaper's work list.
pub fn select_expired(handles: &[SandboxHandle], now: u64) -> Vec<&SandboxHandle> {
    handles.iter().filter(|h| h.expires_at <= now).collect()
}

/// Validate + gate + quota + clamp, then launch via the controller.
pub async fn launch_sandbox(
    controller: &dyn SandboxController,
    quota: &SandboxQuota,
    current_active: u32,
    now: u64,
    requested_ttl_secs: Option<u64>,
    compose_yaml: &str,
) -> Result<SandboxHandle> {
    gate_compose(compose_yaml)?;
    quota_check(current_active, quota)?;
    let ttl = clamp_ttl(requested_ttl_secs, quota);
    let spec = SandboxSpec {
        compose_yaml: compose_yaml.to_string(),
        ttl_secs: ttl,
        expires_at: now + ttl,
    };
    controller.launch(&spec).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota() -> SandboxQuota {
        SandboxQuota { default_ttl_secs: 1800, max_ttl_secs: 7200, max_concurrent_per_user: 2 }
    }

    #[test]
    fn clamp_ttl_defaults_and_caps() {
        let q = quota();
        assert_eq!(clamp_ttl(None, &q), 1800, "unset -> default");
        assert_eq!(clamp_ttl(Some(600), &q), 600, "within range kept");
        assert_eq!(clamp_ttl(Some(99_999), &q), 7200, "over max -> capped");
        assert_eq!(clamp_ttl(Some(0), &q), 1, "zero -> min 1");
    }

    #[test]
    fn quota_check_denies_at_limit() {
        let q = quota();
        assert!(quota_check(0, &q).is_ok());
        assert!(quota_check(1, &q).is_ok());
        assert_eq!(
            quota_check(2, &q),
            Err(AgentToolError::QuotaExceeded(
                "already at 2 concurrent sandboxes (max 2)".into()
            ))
        );
    }

    #[test]
    fn gate_accepts_a_plain_compose() {
        let yaml = "services:\n  web:\n    image: nginx:1.27-alpine\n    ports: [\"8080:80\"]\n";
        assert!(gate_compose(yaml).is_ok());
    }

    #[test]
    fn gate_rejects_privileged() {
        let yaml = "services:\n  x:\n    image: alpine\n    privileged: true\n";
        assert!(matches!(gate_compose(yaml), Err(AgentToolError::ComposeRejected(_))));
    }

    #[test]
    fn gate_rejects_docker_socket_mount() {
        let yaml = "services:\n  x:\n    image: alpine\n    volumes:\n      - /var/run/docker.sock:/var/run/docker.sock\n";
        assert!(matches!(gate_compose(yaml), Err(AgentToolError::ComposeRejected(_))));
    }

    #[test]
    fn gate_rejects_host_networking() {
        let yaml = "services:\n  x:\n    image: alpine\n    network_mode: host\n";
        assert!(matches!(gate_compose(yaml), Err(AgentToolError::ComposeRejected(_))));
    }

    #[test]
    fn gate_rejects_unparsable() {
        assert!(matches!(gate_compose(":\n  bad: ["), Err(AgentToolError::ComposeRejected(_))));
    }

    #[test]
    fn select_expired_picks_past_expiry() {
        let handles = vec![
            SandboxHandle { id: "a".into(), expires_at: 100 },
            SandboxHandle { id: "b".into(), expires_at: 200 },
            SandboxHandle { id: "c".into(), expires_at: 50 },
        ];
        let expired = select_expired(&handles, 150);
        let ids: Vec<&str> = expired.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c"]);
    }

    struct MockController;
    #[async_trait]
    impl SandboxController for MockController {
        async fn launch(&self, spec: &SandboxSpec) -> Result<SandboxHandle> {
            Ok(SandboxHandle { id: "sandbox_test".into(), expires_at: spec.expires_at })
        }
        async fn status(&self, h: &SandboxHandle) -> Result<SandboxStatus> {
            Ok(SandboxStatus {
                id: h.id.clone(),
                health: Health::Healthy,
                live_url: Some("http://1.2.3.4".into()),
                log_excerpt: None,
                expires_at: h.expires_at,
            })
        }
        async fn teardown(&self, _h: &SandboxHandle) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn launch_sandbox_gates_quota_and_clamps() {
        let q = quota();
        let yaml = "services:\n  web:\n    image: nginx:1.27-alpine\n";
        // Happy path: clamps TTL and sets expiry = now + ttl.
        let h = launch_sandbox(&MockController, &q, 0, 1000, Some(99_999), yaml).await.unwrap();
        assert_eq!(h.expires_at, 1000 + 7200);

        // Over quota -> refused before launch.
        assert!(matches!(
            launch_sandbox(&MockController, &q, 2, 1000, None, yaml).await,
            Err(AgentToolError::QuotaExceeded(_))
        ));

        // Unsafe compose -> refused before launch.
        let bad = "services:\n  x:\n    image: alpine\n    privileged: true\n";
        assert!(matches!(
            launch_sandbox(&MockController, &q, 0, 1000, None, bad).await,
            Err(AgentToolError::ComposeRejected(_))
        ));
    }
}
