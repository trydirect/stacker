//! Hetzner Cloud connector.
//!
//! Keep all Hetzner API calls behind this trait so MCP/routes can be tested
//! without touching real infrastructure.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::connectors::ConnectorError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HetznerSnapshotTarget {
    pub provider_server_id: Option<i64>,
    pub server_name: Option<String>,
    pub public_ip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HetznerSnapshot {
    pub action_id: i64,
    pub status: String,
    pub image_id: Option<i64>,
}

/// Request to clone a server from a baked snapshot image — the user-side of the
/// immutable-deploy model. All the fragile work already happened at bake time;
/// deploy = create a server FROM the snapshot and inject cloud-init at first boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HetznerCreateServerRequest {
    pub name: String,
    /// e.g. "cpx11".
    pub server_type: String,
    /// e.g. "fsn1".
    pub location: String,
    /// The baked snapshot's image id (from `HetznerSnapshot.image_id`). In
    /// Hetzner a snapshot IS an image, so it is passed as the `image` field.
    pub image_id: i64,
    pub ssh_key_ids: Vec<i64>,
    /// cloud-init user-data injected at first boot: per-user env, secrets and the
    /// small deterministic config render (domain/TLS). The ONLY per-deploy variance.
    pub user_data: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HetznerProvisionedServer {
    pub id: i64,
    pub public_ipv4: Option<String>,
}

#[async_trait]
pub trait HetznerCloudConnector: Send + Sync {
    async fn create_server_snapshot(
        &self,
        token: &str,
        target: HetznerSnapshotTarget,
        description: &str,
    ) -> Result<HetznerSnapshot, ConnectorError>;

    /// Clone a new server from a baked snapshot image (immutable deploy).
    async fn create_server_from_image(
        &self,
        token: &str,
        request: HetznerCreateServerRequest,
    ) -> Result<HetznerProvisionedServer, ConnectorError>;

    /// List all server-type names. Note: Hetzner's `/server_types` endpoint is
    /// global and does not support a location filter, so this cannot answer
    /// per-region availability — use `validate_server_type_availability` for that.
    async fn list_server_types(&self, token: &str) -> Result<Vec<String>, ConnectorError>;
}

#[derive(Clone)]
pub struct HetznerCloudClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl HetznerCloudClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, ConnectorError> {
        let http_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(ConnectorError::from)?;
        Ok(Self {
            http_client,
            base_url: base_url.into().trim_end_matches("/").to_string(),
        })
    }

    pub fn from_env() -> Result<Self, ConnectorError> {
        let base_url = std::env::var("HETZNER_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.hetzner.cloud/v1".to_string());
        Self::new(base_url)
    }

    async fn resolve_server_id(
        &self,
        token: &str,
        target: &HetznerSnapshotTarget,
    ) -> Result<i64, ConnectorError> {
        if let Some(id) = target.provider_server_id {
            return Ok(id);
        }

        let response = self
            .http_client
            .get(format!("{}/servers", self.base_url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(ConnectorError::from)?;
        let status = response.status();
        if !status.is_success() {
            return Err(status_to_error(status, "Hetzner server lookup failed"));
        }

        let body: HetznerServersResponse = response
            .json()
            .await
            .map_err(|err| ConnectorError::InvalidResponse(err.to_string()))?;
        find_matching_hetzner_server(&body.servers, target)
            .map(|server| server.id)
            .ok_or_else(|| {
                ConnectorError::NotFound(
                    "No Hetzner server matched the saved Stacker server name or public IP"
                        .to_string(),
                )
            })
    }
}

#[async_trait]
impl HetznerCloudConnector for HetznerCloudClient {
    async fn create_server_snapshot(
        &self,
        token: &str,
        target: HetznerSnapshotTarget,
        description: &str,
    ) -> Result<HetznerSnapshot, ConnectorError> {
        let server_id = self.resolve_server_id(token, &target).await?;
        let response = self
            .http_client
            .post(format!(
                "{}/servers/{}/actions/create_image",
                self.base_url, server_id
            ))
            .bearer_auth(token)
            .json(&json!({
                "type": "snapshot",
                "description": description,
            }))
            .send()
            .await
            .map_err(ConnectorError::from)?;

        let status = response.status();
        if !status.is_success() {
            return Err(error_with_body(response, "Hetzner snapshot request failed").await);
        }

        let body: HetznerCreateImageResponse = response
            .json()
            .await
            .map_err(|err| ConnectorError::InvalidResponse(err.to_string()))?;
        let image_id = body
            .action
            .resources
            .iter()
            .find(|resource| resource.resource_type == "image")
            .map(|resource| resource.id);

        Ok(HetznerSnapshot {
            action_id: body.action.id,
            status: body.action.status,
            image_id,
        })
    }

    async fn create_server_from_image(
        &self,
        token: &str,
        request: HetznerCreateServerRequest,
    ) -> Result<HetznerProvisionedServer, ConnectorError> {
        let mut payload = json!({
            "name": request.name,
            "server_type": request.server_type,
            "location": request.location,
            // A snapshot is an image; cloning from it means passing it as `image`.
            "image": request.image_id,
            "start_after_create": true,
        });
        if !request.ssh_key_ids.is_empty() {
            payload["ssh_keys"] = json!(request.ssh_key_ids);
        }
        if let Some(user_data) = &request.user_data {
            payload["user_data"] = json!(user_data);
        }

        let response = self
            .http_client
            .post(format!("{}/servers", self.base_url))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await
            .map_err(ConnectorError::from)?;

        let status = response.status();
        if !status.is_success() {
            return Err(error_with_body(response, "Hetzner create-server-from-image failed").await);
        }

        let body: HetznerCreateServerResponse = response
            .json()
            .await
            .map_err(|err| ConnectorError::InvalidResponse(err.to_string()))?;
        let public_ipv4 = body
            .server
            .public_net
            .and_then(|net| net.ipv4)
            .map(|ipv4| ipv4.ip);
        Ok(HetznerProvisionedServer {
            id: body.server.id,
            public_ipv4,
        })
    }

    async fn list_server_types(&self, token: &str) -> Result<Vec<String>, ConnectorError> {
        let url = format!("{}/server_types", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(ConnectorError::from)?;

        let status = response.status();
        if !status.is_success() {
            return Err(status_to_error(
                status,
                "Hetzner server types lookup failed",
            ));
        }

        let body: HetznerServerTypesResponse = response
            .json()
            .await
            .map_err(|err| ConnectorError::InvalidResponse(err.to_string()))?;

        Ok(body.server_types.into_iter().map(|t| t.name).collect())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Server-type × region availability validation
//
// Shared by the deploy route handler AND the CLI local-orchestrator path so the
// pre-flight guard is identical in both. `/server_types` is global and does NOT
// honor a `?location=` filter — per-region availability comes from `/datacenters`
// (`server_types.available` lists the type ids creatable in each datacenter).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Resolve the Hetzner API base URL, honoring `STACKER_HETZNER_API_URL` (used by
/// tests to point at a mock) then falling back to the public API.
pub fn api_base_url() -> String {
    std::env::var("STACKER_HETZNER_API_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "https://api.hetzner.cloud/v1".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Look up the public IPv4 of a Hetzner server by its name.
///
/// Used to reconcile `server.srv_ip` when a deploy provisioned a server but the
/// install service never reported the IP back (deployment ends `paused`/`failed`
/// with `srv_ip` null). Hetzner assigns a public IPv4 at creation, so the IP is
/// available on the provider side even when the later Ansible step failed.
///
/// Returns `Ok(None)` when no server matches the name or the match has no IPv4
/// yet; `Err` only on transport/HTTP/parse failure so callers can decide whether
/// to retry.
pub async fn fetch_server_ipv4_by_name(
    base_url: &str,
    token: &str,
    name: &str,
) -> Result<Option<String>, ConnectorError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(ConnectorError::from)?;

    let response = client
        .get(format!("{}/servers", base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map_err(ConnectorError::from)?;

    let status = response.status();
    if !status.is_success() {
        return Err(status_to_error(status, "Hetzner server lookup failed"));
    }

    let body: HetznerServersResponse = response
        .json()
        .await
        .map_err(|err| ConnectorError::InvalidResponse(err.to_string()))?;

    Ok(body
        .servers
        .iter()
        .find(|server| server.name == name)
        .and_then(hetzner_server_ip)
        .map(str::to_string))
}

/// Validate that `server_type` can be created in `region` on Hetzner.
///
/// Fails open (returns `Ok`) on any transport/HTTP/parse error so a Hetzner
/// outage never blocks deploys. Returns `Err(msg)` only when we positively know
/// the type is unknown, deprecated, or not offered in a region we can see.
pub async fn validate_server_type_availability(
    base_url: &str,
    token: &str,
    server_type: &str,
    region: Option<&str>,
) -> Result<(), String> {
    let server_type = server_type.trim();
    if server_type.is_empty() || token.trim().is_empty() {
        return Ok(());
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!("Could not initialize Hetzner API client: {}; proceeding", err);
            return Ok(());
        }
    };

    let server_types = match fetch_hetzner_json::<HetznerServerTypesResponse>(
        &client,
        &format!("{}/server_types", base_url),
        token,
        "server types",
    )
    .await
    {
        Some(body) => body.server_types,
        None => return Ok(()),
    };

    // Datacenters are best-effort: without them we skip the region check but
    // still enforce existence + deprecation.
    let datacenters = fetch_hetzner_json::<HetznerDatacentersResponse>(
        &client,
        &format!("{}/datacenters", base_url),
        token,
        "datacenters",
    )
    .await
    .map(|body| body.datacenters)
    .unwrap_or_default();

    evaluate_server_type_availability(&server_types, &datacenters, server_type, region)
}

/// GET a Hetzner JSON endpoint, returning `None` (and logging a warning) on any
/// transport, HTTP, or deserialization error so callers can fail open.
async fn fetch_hetzner_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    what: &str,
) -> Option<T> {
    let response = match client.get(url).bearer_auth(token).send().await {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("Could not reach Hetzner API for {}: {}; proceeding", what, err);
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(
            "Hetzner {} API returned HTTP {}; skipping server type validation",
            what,
            response.status().as_u16()
        );
        return None;
    }

    match response.json::<T>().await {
        Ok(body) => Some(body),
        Err(err) => {
            tracing::warn!("Invalid Hetzner {} response: {}; skipping validation", what, err);
            None
        }
    }
}

/// Pure availability check, split out from I/O so it is unit-testable.
///
/// - Unknown type name → error (not offered by Hetzner at all).
/// - Deprecated type → error (cannot create new servers).
/// - `region` given and known to `/datacenters` but not offering the type →
///   error naming the region. This is the case region-blind validation missed:
///   `/server_types?location=…` is ignored by Hetzner, so a globally-existing
///   type like `cpx21` falsely passed even when unavailable in e.g. `nbg1`.
/// - `region` unknown to `/datacenters` (or datacenters unavailable) → fail open.
fn evaluate_server_type_availability(
    server_types: &[HetznerServerTypeEntry],
    datacenters: &[HetznerDatacenterEntry],
    server_type: &str,
    region: Option<&str>,
) -> Result<(), String> {
    use std::collections::HashSet;

    let active_type_names = |ids: Option<&HashSet<i64>>| -> String {
        let names: Vec<&str> = server_types
            .iter()
            .filter(|t| t.deprecated.is_none())
            .filter(|t| ids.map_or(true, |set| set.contains(&t.id)))
            .map(|t| t.name.as_str())
            .collect();
        if names.is_empty() {
            "none found".to_string()
        } else {
            names.join(", ")
        }
    };

    let entry = match server_types
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(server_type))
    {
        Some(entry) => entry,
        None => {
            return Err(format!(
                "Server type '{}' is not available in Hetzner. Available types: {}",
                server_type,
                active_type_names(None)
            ));
        }
    };

    if entry.deprecated.is_some() {
        return Err(format!(
            "Server type '{}' is deprecated in Hetzner and can no longer be used to create new servers. \
             Set `deploy.cloud.size` in stacker.yml to an active type: {}",
            server_type,
            active_type_names(None)
        ));
    }

    // Per-region availability check — only meaningful when we know the region
    // AND `/datacenters` actually lists it. Otherwise fail open.
    if let Some(region) = region.map(str::trim).filter(|r| !r.is_empty()) {
        let region_dcs: Vec<&HetznerDatacenterEntry> = datacenters
            .iter()
            .filter(|dc| dc.location.name.eq_ignore_ascii_case(region))
            .collect();

        if !region_dcs.is_empty() {
            let available_ids: HashSet<i64> = region_dcs
                .iter()
                .flat_map(|dc| dc.server_types.available.iter().copied())
                .collect();

            if !available_ids.contains(&entry.id) {
                return Err(format!(
                    "Server type '{}' is not available in Hetzner location '{}'. \
                     Set `deploy.cloud.region` or `deploy.cloud.size` in stacker.yml. \
                     Types available in '{}': {}",
                    server_type,
                    region,
                    region,
                    active_type_names(Some(&available_ids))
                ));
            }
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct HetznerServerTypesResponse {
    #[serde(default)]
    server_types: Vec<HetznerServerTypeEntry>,
}

#[derive(Debug, Deserialize)]
struct HetznerServerTypeEntry {
    /// Numeric id — what `/datacenters` lists under `server_types.available`.
    id: i64,
    name: String,
    /// Non-null when Hetzner has deprecated this type (ISO-8601 timestamp).
    #[serde(default)]
    deprecated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HetznerDatacentersResponse {
    #[serde(default)]
    datacenters: Vec<HetznerDatacenterEntry>,
}

#[derive(Debug, Deserialize)]
struct HetznerDatacenterEntry {
    location: HetznerDatacenterLocation,
    #[serde(default)]
    server_types: HetznerDatacenterServerTypes,
}

#[derive(Debug, Deserialize)]
struct HetznerDatacenterLocation {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct HetznerDatacenterServerTypes {
    /// Server-type ids creatable in this datacenter.
    #[serde(default)]
    available: Vec<i64>,
}

fn status_to_error(status: reqwest::StatusCode, message: &str) -> ConnectorError {
    match status.as_u16() {
        // 401 is genuine auth; 403 is forbidden for many reasons (permissions,
        // resource limits, protection) — do NOT claim "rejected token" for it.
        401 => ConnectorError::Unauthorized("Hetzner rejected the API token (401)".to_string()),
        403 => ConnectorError::HttpError(format!(
            "{message} forbidden (403) — check token permissions or account/resource limits"
        )),
        404 => ConnectorError::NotFound(message.to_string()),
        429 => ConnectorError::RateLimited("Hetzner API rate limit exceeded".to_string()),
        _ => ConnectorError::HttpError(format!("{} with status {}", message, status.as_u16())),
    }
}

/// Build an error that INCLUDES Hetzner's response body, so the real cause
/// (e.g. `resource_limit_exceeded: image limit exceeded`) is surfaced instead of
/// a misleading generic status message.
async fn error_with_body(response: reqwest::Response, context: &str) -> ConnectorError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let snippet = body.trim();
    match status.as_u16() {
        401 => ConnectorError::Unauthorized(format!("{context}: {snippet}")),
        429 => ConnectorError::RateLimited(format!("{context}: {snippet}")),
        404 => ConnectorError::NotFound(format!("{context}: {snippet}")),
        _ => ConnectorError::HttpError(format!("{context}: HTTP {}: {snippet}", status.as_u16())),
    }
}

fn find_matching_hetzner_server<'a>(
    servers: &'a [HetznerServer],
    target: &HetznerSnapshotTarget,
) -> Option<&'a HetznerServer> {
    let expected_ip = target
        .public_ip
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let expected_name = target
        .server_name
        .as_deref()
        .filter(|value| !value.trim().is_empty());

    servers.iter().find(|server| {
        expected_ip.is_some_and(|ip| hetzner_server_ip(server) == Some(ip))
            || expected_name.is_some_and(|name| server.name == name)
    })
}

fn hetzner_server_ip(server: &HetznerServer) -> Option<&str> {
    server
        .public_net
        .as_ref()
        .and_then(|net| net.ipv4.as_ref())
        .map(|ipv4| ipv4.ip.as_str())
}

#[derive(Debug, Deserialize)]
struct HetznerServersResponse {
    servers: Vec<HetznerServer>,
}

#[derive(Debug, Deserialize)]
struct HetznerServer {
    id: i64,
    name: String,
    #[serde(default)]
    public_net: Option<HetznerServerPublicNet>,
}

#[derive(Debug, Deserialize)]
struct HetznerServerPublicNet {
    #[serde(default)]
    ipv4: Option<HetznerServerIpv4>,
}

#[derive(Debug, Deserialize)]
struct HetznerServerIpv4 {
    ip: String,
}

#[derive(Debug, Deserialize)]
struct HetznerCreateImageResponse {
    action: HetznerAction,
}

#[derive(Debug, Deserialize)]
struct HetznerCreateServerResponse {
    server: HetznerServer,
}

#[derive(Debug, Deserialize)]
struct HetznerAction {
    id: i64,
    status: String,
    #[serde(default)]
    resources: Vec<HetznerActionResource>,
}

#[derive(Debug, Deserialize)]
struct HetznerActionResource {
    id: i64,
    #[serde(rename = "type")]
    resource_type: String,
}


#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn stype(id: i64, name: &str, deprecated: Option<&str>) -> HetznerServerTypeEntry {
        HetznerServerTypeEntry {
            id,
            name: name.to_string(),
            deprecated: deprecated.map(ToOwned::to_owned),
        }
    }

    fn datacenter(location: &str, available: &[i64]) -> HetznerDatacenterEntry {
        HetznerDatacenterEntry {
            location: HetznerDatacenterLocation {
                name: location.to_string(),
            },
            server_types: HetznerDatacenterServerTypes {
                available: available.to_vec(),
            },
        }
    }

    // The regression from the bug report: `cpx21` exists globally, so the old
    // `/server_types?location=…` check falsely passed, but Hetzner does not
    // offer it in `nbg1`, so Terraform later died with "unsupported location".
    #[test]
    fn server_type_unavailable_in_region_is_rejected() {
        let types = vec![stype(22, "cpx11", None), stype(23, "cpx21", None)];
        let dcs = vec![
            datacenter("fsn1", &[22, 23]),
            datacenter("nbg1", &[22]), // cpx21 (id 23) NOT available here
        ];

        let err = evaluate_server_type_availability(&types, &dcs, "cpx21", Some("nbg1"))
            .expect_err("cpx21 must be rejected in nbg1");
        assert!(err.contains("cpx21"), "error should name the type: {err}");
        assert!(err.contains("nbg1"), "error should name the region: {err}");
        assert!(err.contains("cpx11"), "error should list available types: {err}");
    }

    #[test]
    fn server_type_available_in_region_passes() {
        let types = vec![stype(22, "cpx11", None), stype(23, "cpx21", None)];
        let dcs = vec![datacenter("fsn1", &[22, 23])];
        assert!(evaluate_server_type_availability(&types, &dcs, "cpx21", Some("fsn1")).is_ok());
    }

    #[test]
    fn region_matching_is_case_insensitive() {
        let types = vec![stype(23, "cpx21", None)];
        let dcs = vec![datacenter("fsn1", &[23])];
        assert!(evaluate_server_type_availability(&types, &dcs, "CPX21", Some("FSN1")).is_ok());
    }

    #[test]
    fn deprecated_server_type_is_rejected() {
        let types = vec![
            stype(1, "cx11", Some("2024-01-01T00:00:00+00:00")),
            stype(22, "cpx11", None),
        ];
        let dcs = vec![datacenter("fsn1", &[1, 22])];
        let err = evaluate_server_type_availability(&types, &dcs, "cx11", Some("fsn1"))
            .expect_err("deprecated type must be rejected");
        assert!(err.contains("deprecated"), "err: {err}");
        assert!(err.contains("cpx11"), "should suggest active type: {err}");
    }

    #[test]
    fn unknown_server_type_is_rejected() {
        let types = vec![stype(22, "cpx11", None)];
        let dcs = vec![datacenter("fsn1", &[22])];
        let err = evaluate_server_type_availability(&types, &dcs, "does-not-exist", Some("fsn1"))
            .expect_err("unknown type must be rejected");
        assert!(err.contains("does-not-exist"), "err: {err}");
    }

    // Fail-open guarantees: no region, unknown region, and missing datacenter
    // data must never block a deploy on the region dimension.
    #[test]
    fn no_region_skips_region_check() {
        let types = vec![stype(23, "cpx21", None)];
        assert!(evaluate_server_type_availability(&types, &[], "cpx21", None).is_ok());
    }

    #[test]
    fn unknown_region_fails_open() {
        let types = vec![stype(23, "cpx21", None)];
        let dcs = vec![datacenter("fsn1", &[23])];
        // Region not present in /datacenters — we cannot prove unavailability.
        assert!(evaluate_server_type_availability(&types, &dcs, "cpx21", Some("ash")).is_ok());
    }

    #[test]
    fn empty_datacenters_fails_open_on_region() {
        let types = vec![stype(23, "cpx21", None)];
        // Simulates /datacenters fetch failure: existence still checked, region skipped.
        assert!(evaluate_server_type_availability(&types, &[], "cpx21", Some("nbg1")).is_ok());
    }

    // End-to-end through the HTTP layer against a mock Hetzner, exercising the
    // /server_types + /datacenters fetch and fail-open on the datacenters call.
    #[tokio::test]
    async fn validate_rejects_type_missing_in_region_via_http() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/server_types"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "server_types": [
                    {"id": 22, "name": "cpx11"},
                    {"id": 23, "name": "cpx21"}
                ]
            })))
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/datacenters"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "datacenters": [
                    {"location": {"name": "nbg1"}, "server_types": {"available": [22]}}
                ]
            })))
            .mount(&api)
            .await;

        let err = validate_server_type_availability(&api.uri(), "tok", "cpx21", Some("nbg1"))
            .await
            .expect_err("cpx21 not in nbg1");
        assert!(err.contains("nbg1"), "err: {err}");
    }

    #[tokio::test]
    async fn fetch_server_ipv4_by_name_returns_ip_for_match() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/servers"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "servers": [
                    {"id": 1, "name": "other", "public_net": {"ipv4": {"ip": "1.1.1.1"}}},
                    {"id": 2, "name": "nocodb-419", "public_net": {"ipv4": {"ip": "203.0.113.7"}}}
                ]
            })))
            .mount(&api)
            .await;

        let ip = fetch_server_ipv4_by_name(&api.uri(), "tok", "nocodb-419")
            .await
            .unwrap();
        assert_eq!(ip.as_deref(), Some("203.0.113.7"));
    }

    #[tokio::test]
    async fn fetch_server_ipv4_by_name_returns_none_when_absent() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/servers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "servers": [{"id": 1, "name": "other", "public_net": {"ipv4": {"ip": "1.1.1.1"}}}]
            })))
            .mount(&api)
            .await;

        let ip = fetch_server_ipv4_by_name(&api.uri(), "tok", "missing")
            .await
            .unwrap();
        assert!(ip.is_none());
    }

    #[tokio::test]
    async fn validate_fails_open_when_datacenters_unavailable() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/server_types"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "server_types": [{"id": 23, "name": "cpx21"}]
            })))
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/datacenters"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&api)
            .await;

        // Existence passes, region check skipped → Ok.
        assert!(validate_server_type_availability(&api.uri(), "tok", "cpx21", Some("nbg1"))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn create_snapshot_resolves_server_by_public_ip_without_live_api() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/servers"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "servers": [{
                    "id": 123,
                    "name": "prod-web-1",
                    "public_net": { "ipv4": { "ip": "203.0.113.10" } }
                }]
            })))
            .mount(&api)
            .await;
        Mock::given(method("POST"))
            .and(path("/servers/123/actions/create_image"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_partial_json(json!({"type": "snapshot"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "action": {
                    "id": 777,
                    "status": "running",
                    "resources": [{"id": 888, "type": "image"}]
                }
            })))
            .mount(&api)
            .await;

        let client = HetznerCloudClient::new(api.uri()).unwrap();
        let snapshot = client
            .create_server_snapshot(
                "test-token",
                HetznerSnapshotTarget {
                    provider_server_id: None,
                    server_name: None,
                    public_ip: Some("203.0.113.10".to_string()),
                },
                "Stacker troubleshooting snapshot",
            )
            .await
            .unwrap();

        assert_eq!(snapshot.action_id, 777);
        assert_eq!(snapshot.image_id, Some(888));
        assert_eq!(snapshot.status, "running");
    }

    #[tokio::test]
    async fn create_snapshot_can_use_known_provider_server_id() {
        let api = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/servers/456/actions/create_image"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "action": { "id": 778, "status": "running", "resources": [] }
            })))
            .mount(&api)
            .await;

        let client = HetznerCloudClient::new(api.uri()).unwrap();
        let snapshot = client
            .create_server_snapshot(
                "test-token",
                HetznerSnapshotTarget {
                    provider_server_id: Some(456),
                    server_name: None,
                    public_ip: None,
                },
                "Stacker troubleshooting snapshot",
            )
            .await
            .unwrap();

        assert_eq!(snapshot.action_id, 778);
        assert_eq!(snapshot.image_id, None);
    }

    #[tokio::test]
    async fn list_server_types_returns_names() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/server_types"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "server_types": [
                    {"id": 1, "name": "cx22"},
                    {"id": 2, "name": "cx32"},
                    {"id": 3, "name": "cx42"}
                ]
            })))
            .mount(&api)
            .await;

        let client = HetznerCloudClient::new(api.uri()).unwrap();
        let types = client.list_server_types("test-token").await.unwrap();

        assert_eq!(types, vec!["cx22", "cx32", "cx42"]);
    }

    #[tokio::test]
    async fn list_server_types_returns_unauthorized_on_401() {
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/server_types"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&api)
            .await;

        let client = HetznerCloudClient::new(api.uri()).unwrap();
        let result = client.list_server_types("bad-token").await;

        assert!(matches!(result, Err(ConnectorError::Unauthorized(_))));
    }
}
