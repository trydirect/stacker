//! MCP tool `deploy_ephemeral` — give an agent *hands*: run an arbitrary
//! docker-compose on a throwaway box and get back a live URL + logs + health,
//! auto-torn-down after a TTL.
//!
//! The safety/quota/TTL policy is the pure, tested `agent_tools::sandbox` core.
//! The actual provisioning is the [`SandboxController`] seam; [`FreshVmController`]
//! is the production impl over the existing deploy path (wired in M3 — until then
//! it reports that ephemeral provisioning is not enabled, but the tool still
//! enforces the compose safety-gate and per-user quota).

use async_trait::async_trait;
use serde_json::{json, Value};

use agent_tools::error::{AgentToolError, Result as AgentResult};
use agent_tools::sandbox::{
    launch_sandbox, SandboxController, SandboxHandle, SandboxQuota, SandboxSpec, SandboxStatus,
};

use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};

/// Read the sandbox quota from `SANDBOX_*` env, falling back to safe defaults.
fn quota_from_env() -> SandboxQuota {
    let d = SandboxQuota::default();
    let num = |k: &str, dflt: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(dflt);
    SandboxQuota {
        default_ttl_secs: num("SANDBOX_DEFAULT_TTL_SECS", d.default_ttl_secs),
        max_ttl_secs: num("SANDBOX_MAX_TTL_SECS", d.max_ttl_secs),
        max_concurrent_per_user: std::env::var("SANDBOX_MAX_CONCURRENT_PER_USER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d.max_concurrent_per_user),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Production sandbox controller over the existing OpenTofu→Hetzner deploy path.
/// Provisioning is wired in M3; the trait shape is stable so the tool + tests
/// don't change when live provisioning (or the v2 Kata pool) lands.
pub struct FreshVmController {
    owner: String,
}

impl FreshVmController {
    pub fn new(owner: String) -> Self {
        FreshVmController { owner }
    }
}

#[async_trait]
impl SandboxController for FreshVmController {
    async fn active_count(&self, _owner: &str) -> AgentResult<u32> {
        // M3: count non-expired deployments with hash prefix "sandbox_" for the
        // owner via db::deployment. Zero until provisioning is enabled.
        Ok(0)
    }

    async fn launch(&self, _spec: &SandboxSpec) -> AgentResult<SandboxHandle> {
        Err(AgentToolError::Backend(format!(
            "ephemeral provisioning is not enabled on this instance yet (owner={}); \
             the compose passed the safety-gate and quota checks",
            self.owner
        )))
    }

    async fn status(&self, _handle: &SandboxHandle) -> AgentResult<SandboxStatus> {
        Err(AgentToolError::Backend("sandbox status is not available yet".into()))
    }

    async fn teardown(&self, _handle: &SandboxHandle) -> AgentResult<()> {
        Err(AgentToolError::Backend("sandbox teardown is not available yet".into()))
    }
}

pub struct DeployEphemeralTool;

#[async_trait]
impl ToolHandler for DeployEphemeralTool {
    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolContent, String> {
        let compose = args
            .get("compose_yaml")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "`compose_yaml` (a docker-compose.yml) is required".to_string())?;
        let requested_ttl = args.get("ttl_secs").and_then(|v| v.as_u64());

        let quota = quota_from_env();
        let owner = context.user.id.clone();
        let controller = FreshVmController::new(owner.clone());

        // Full tested path: safety-gate → quota → TTL clamp → launch.
        let active = controller.active_count(&owner).await.map_err(|e| e.to_string())?;
        let handle = launch_sandbox(&controller, &quota, active, now_secs(), requested_ttl, compose)
            .await
            .map_err(|e| e.to_string())?;

        let json = json!({
            "id": handle.id,
            "expires_at": handle.expires_at,
            "poll": "call sandbox status with this id for the live_url once healthy",
        });
        Ok(ToolContent::Text {
            text: serde_json::to_string_pretty(&json).unwrap_or_default(),
        })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "deploy_ephemeral".to_string(),
            description: "Run a docker-compose.yml on a throwaway cloud sandbox and get back a live URL, logs and health. The sandbox auto-tears-down after ttl_secs. The compose is safety-gated (no privileged, Docker socket, or host networking) and subject to a per-user concurrency quota. Use this to actually SEE a stack run instead of guessing whether it works.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "compose_yaml": {
                        "type": "string",
                        "description": "A docker-compose.yml to run in the sandbox."
                    },
                    "ttl_secs": {
                        "type": "integer",
                        "description": "Requested lifetime in seconds (clamped to the server max, default ~30m)."
                    }
                },
                "required": ["compose_yaml"]
            }),
        }
    }
}
