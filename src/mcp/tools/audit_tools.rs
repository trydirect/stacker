//! MCP tools wrapping the pure `td_audit` verification engines. These give an
//! agent "check before you commit" capabilities on the artifacts it generates —
//! all working today, no infra, no auth. Descriptions are trigger-first (they
//! tell the model *when* to call), which is what makes an agent actually pick
//! them.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};

/// Extract the `content` argument (the file text to analyze).
fn content_arg(args: &Value, what: &str) -> Result<String, String> {
    args.get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("`content` (the {what} text) is required"))
}

fn report_json<T: Serialize>(report: &T) -> Result<ToolContent, String> {
    Ok(ToolContent::Text {
        text: serde_json::to_string_pretty(report).map_err(|e| e.to_string())?,
    })
}

fn content_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "properties": { "content": { "type": "string", "description": desc } },
        "required": ["content"]
    })
}

// ── Compose auditor ─────────────────────────────────────────────────────────
pub struct AuditComposeTool;

#[async_trait]
impl ToolHandler for AuditComposeTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolContent, String> {
        let content = content_arg(&args, "docker-compose.yml")?;
        report_json(&td_audit::compose::audit_compose(&content))
    }
    fn schema(&self) -> Tool {
        Tool {
            name: "audit_compose".to_string(),
            description: "Call this right after generating or editing a docker-compose.yml, before deploying it: returns a graded report of security and best-practice issues (hardcoded secrets, unpinned/`latest` images, privileged containers, missing healthchecks/resource limits, exposed ports). Fix the findings before you ship.".to_string(),
            input_schema: content_schema("The full docker-compose.yml text."),
        }
    }
}

// ── Dockerfile linter ───────────────────────────────────────────────────────
pub struct AuditDockerfileTool;

#[async_trait]
impl ToolHandler for AuditDockerfileTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolContent, String> {
        let content = content_arg(&args, "Dockerfile")?;
        report_json(&td_audit::dockerfile::audit_dockerfile(&content))
    }
    fn schema(&self) -> Tool {
        Tool {
            name: "audit_dockerfile".to_string(),
            description: "Call this after writing or editing a Dockerfile, before building: returns a graded report of issues (unpinned base image, running as root, secrets baked into ENV/ARG, `ADD` of local files, missing HEALTHCHECK). Fix them before you build the image.".to_string(),
            input_schema: content_schema("The full Dockerfile text."),
        }
    }
}

// ── Exposure audit ──────────────────────────────────────────────────────────
pub struct AuditExposureTool;

#[async_trait]
impl ToolHandler for AuditExposureTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolContent, String> {
        let content = content_arg(&args, "docker-compose.yml")?;
        report_json(&td_audit::exposure::audit_exposure(&content))
    }
    fn schema(&self) -> Tool {
        Tool {
            name: "audit_exposure".to_string(),
            description: "Call this on a docker-compose.yml before exposing it to a network: flags services (databases, admin UIs) published to the public internet vs bound to localhost, and recommends safe port bindings. Use it to avoid accidentally exposing a database.".to_string(),
            input_schema: content_schema("The full docker-compose.yml text."),
        }
    }
}

// ── Production-readiness score ──────────────────────────────────────────────
pub struct AuditReadinessTool;

#[async_trait]
impl ToolHandler for AuditReadinessTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolContent, String> {
        let content = content_arg(&args, "docker-compose.yml")?;
        report_json(&td_audit::readiness::audit_readiness(&content))
    }
    fn schema(&self) -> Tool {
        Tool {
            name: "audit_readiness".to_string(),
            description: "Call this on a docker-compose.yml before calling a stack 'done': one composite production-readiness grade over security, exposure and operational hygiene (restart policies, healthchecks, resource limits). Use it to decide whether a stack is safe to deploy.".to_string(),
            input_schema: content_schema("The full docker-compose.yml text."),
        }
    }
}

// ── Multi-cloud cost estimate ───────────────────────────────────────────────
pub struct EstimateCostTool;

#[async_trait]
impl ToolHandler for EstimateCostTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolContent, String> {
        let content = content_arg(&args, "docker-compose.yml")?;
        report_json(&td_audit::cost::estimate_cost(&content, &td_audit::cost::DefaultPricing))
    }
    fn schema(&self) -> Tool {
        Tool {
            name: "estimate_cost".to_string(),
            description: "Call this when the user asks what a stack will cost, or before recommending where to host it: sizes a docker-compose.yml and returns the cheapest fitting instance across Hetzner, DigitalOcean, Linode, Vultr and AWS. Use it to give a concrete monthly price and the cheapest provider.".to_string(),
            input_schema: content_schema("The full docker-compose.yml text."),
        }
    }
}
