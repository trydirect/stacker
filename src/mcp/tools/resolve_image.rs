//! MCP tool `resolve_image` — ground truth about a Docker image reference.
//!
//! The decision/assembly logic lives in the pure `agent_tools::image` core; this
//! module provides the real [`ImageResolver`] over the Docker Hub v2 API and the
//! `ToolHandler` wrapper. An agent calls this to stop guessing whether an image
//! exists, is multi-arch, is pinned, etc. (the class of hallucination that
//! produced `trydirect/redis`).

use async_trait::async_trait;
use serde_json::{json, Value};

use agent_tools::image::{resolve_image, ImageResolver, ParsedRef, RawImageFacts, ResolvedImage};

use crate::mcp::protocol::{Tool, ToolContent};
use crate::mcp::registry::{ToolContext, ToolHandler};

const HUB: &str = "https://hub.docker.com/v2";

/// Resolve a reference to ground-truth facts. Shared by the MCP tool and the
/// public (unauthenticated) HTTP endpoint so both behave identically.
pub async fn resolve_reference(reference: &str) -> Result<ResolvedImage, String> {
    let resolver = DockerHubImageResolver::new();
    // CVE summary is a follow-up (Trivy); ground-truth metadata now.
    resolve_image(&resolver, reference, None)
        .await
        .map_err(|e| e.to_string())
}

/// Docker Hub-backed resolver. Only ever contacts hub.docker.com (no SSRF
/// surface: the reference is normalized to a `namespace/repo` path).
pub struct DockerHubImageResolver {
    http: reqwest::Client,
}

impl DockerHubImageResolver {
    pub fn new() -> Self {
        DockerHubImageResolver {
            http: reqwest::Client::builder()
                .user_agent(concat!("stacker-agent-gateway/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    async fn get_json(&self, url: &str) -> Option<Value> {
        let resp = self.http.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<Value>().await.ok()
    }
}

impl Default for DockerHubImageResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Rough days-since for an ISO-8601 timestamp (no chrono dep), mirroring the
/// audit route's helper.
fn days_since_iso8601(ts: &str) -> Option<u64> {
    let year: i64 = ts.get(0..4)?.parse().ok()?;
    let month: i64 = ts.get(5..7)?.parse().ok()?;
    let day: i64 = ts.get(8..10)?.parse().ok()?;
    let to_days = |y: i64, m: i64, d: i64| y * 365 + (y / 4) + m * 30 + d;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64
        / 86_400
        + to_days(1970, 1, 1);
    Some((now - to_days(year, month, day)).max(0) as u64)
}

#[async_trait]
impl ImageResolver for DockerHubImageResolver {
    async fn facts(&self, parsed: &ParsedRef) -> agent_tools::error::Result<RawImageFacts> {
        let repo = &parsed.repo;

        // 1. Repository: existence + last push.
        let repo_json = self.get_json(&format!("{HUB}/repositories/{repo}/")).await;
        let exists = repo_json.is_some();
        let last_pushed = repo_json
            .as_ref()
            .and_then(|v| v.get("last_updated").and_then(|d| d.as_str()).map(String::from));
        let last_updated_days = last_pushed.as_deref().and_then(days_since_iso8601);

        if !exists {
            return Ok(RawImageFacts { exists: false, ..Default::default() });
        }

        // 2. Recent tags (best-effort).
        let recent_tags = self
            .get_json(&format!("{HUB}/repositories/{repo}/tags?page_size=10&ordering=last_updated"))
            .await
            .and_then(|v| v.get("results").and_then(|r| r.as_array()).cloned())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // 3. Per-tag images → architectures + size + digest.
        let tag = parsed.tag.as_deref().unwrap_or("latest");
        let images = self
            .get_json(&format!("{HUB}/repositories/{repo}/tags/{tag}/images"))
            .await;
        let (architectures, size_bytes, digest) = match images.and_then(|v| v.as_array().cloned()) {
            Some(arr) => {
                let mut archs = Vec::new();
                let mut size = None;
                let mut dig = parsed.digest.clone();
                for img in &arr {
                    if let Some(a) = img.get("architecture").and_then(|a| a.as_str()) {
                        let variant = img.get("variant").and_then(|x| x.as_str());
                        let label = match variant {
                            Some(v) if !v.is_empty() => format!("{a}/{v}"),
                            _ => a.to_string(),
                        };
                        if !archs.contains(&label) {
                            archs.push(label);
                        }
                    }
                    if size.is_none() {
                        size = img.get("size").and_then(|s| s.as_u64());
                    }
                    if dig.is_none() {
                        dig = img.get("digest").and_then(|d| d.as_str()).map(String::from);
                    }
                }
                (archs, size, dig)
            }
            None => (Vec::new(), None, parsed.digest.clone()),
        };

        Ok(RawImageFacts {
            exists: true,
            digest,
            size_bytes,
            architectures,
            recent_tags,
            last_pushed,
            last_updated_days,
        })
    }
}

pub struct ResolveImageTool;

#[async_trait]
impl ToolHandler for ResolveImageTool {
    async fn execute(&self, args: Value, _context: &ToolContext) -> Result<ToolContent, String> {
        let reference = args
            .get("reference")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "`reference` (an image ref, e.g. \"redis:7-alpine\") is required".to_string())?;

        let resolved = resolve_reference(reference).await?;
        let json = serde_json::to_string_pretty(&resolved).map_err(|e| e.to_string())?;
        Ok(ToolContent::Text { text: json })
    }

    fn schema(&self) -> Tool {
        Tool {
            name: "resolve_image".to_string(),
            description: "Ground truth about a Docker image reference: whether it exists, its digest, size, supported architectures, recent tags, last push date, whether it is official/pinned, a quality grade, and (when enabled) a CVE summary. Use this before writing a compose/Dockerfile to avoid referencing an image that does not exist or is not multi-arch.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "reference": {
                        "type": "string",
                        "description": "Image reference, e.g. \"nginx\", \"redis:7-alpine\", \"ghcr.io/owner/repo:v2\", or \"name@sha256:...\"."
                    }
                },
                "required": ["reference"]
            }),
        }
    }
}
