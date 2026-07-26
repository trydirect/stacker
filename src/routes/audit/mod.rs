//! Public "audit checker" endpoints under `/api/audit/*`.
//!
//! Thin actix wrappers over the pure `td_audit` engines. Unauthenticated
//! (granted to `group_anonymous` in casbin); paste-based checkers take the raw
//! body as text. Image inspection fetches Docker Hub metadata only (no
//! arbitrary host — no SSRF surface here) and leaves CVE scanning to a future
//! `VulnScanner` (Trivy) integration.

use actix_web::web::ServiceConfig;
use actix_web::{post, web, HttpResponse, Responder};
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use serde::Deserialize;

use crate::helpers::rate_limit::AuditRateLimitConfig;
use crate::middleware::rate_limit::AuditRateLimit;

use td_audit::compose::audit_compose;
use td_audit::cost::{estimate_cost, DefaultPricing};
use td_audit::dockerfile::audit_dockerfile;
use td_audit::exposure::audit_exposure;
use td_audit::image::{
    audit_image, parse_trivy_report, ImageInfo, ImageMetadata, VulnScanner, Vulnerability,
};
use td_audit::readiness::audit_readiness;

fn register(cfg: &mut ServiceConfig) {
    cfg.service(compose)
        .service(dockerfile)
        .service(exposure)
        .service(readiness)
        .service(cost)
        .service(image);
}

/// Mount `/api/audit/*` with a body-size cap and, when Redis is available, the
/// shared per-IP/global rate limiter. Used as
/// `App::new().configure(|c| routes::audit::configure(c, redis, cfg))`.
pub fn configure(cfg: &mut ServiceConfig, redis: Option<ConnectionManager>, rl: AuditRateLimitConfig) {
    let body_cap = rl.max_body_kb * 1024;
    let state = web::Data::new(AuditState {
        redis: redis.clone(),
        cache_ttl: rl.cache_ttl_secs,
    });
    match redis {
        Some(conn) => {
            cfg.service(
                web::scope("/api/audit")
                    .app_data(web::PayloadConfig::new(body_cap))
                    .app_data(state)
                    .wrap(AuditRateLimit::new(conn, rl))
                    .configure(register),
            );
        }
        None => {
            cfg.service(
                web::scope("/api/audit")
                    .app_data(web::PayloadConfig::new(body_cap))
                    .app_data(state)
                    .configure(register),
            );
        }
    }
}

/// Per-scope runtime for handlers: shared Redis (for result caching) + TTL.
#[derive(Clone)]
struct AuditState {
    redis: Option<ConnectionManager>,
    cache_ttl: u64,
}

fn json_response(body: String, cache: &str) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .insert_header(("X-Audit-Cache", cache))
        .body(body)
}

/// Return a cached JSON response for identical input, else compute (sync engine),
/// cache and return. `X-Audit-Cache: HIT|MISS|BYPASS`.
async fn cached_json(
    state: &AuditState,
    checker: &str,
    body: &[u8],
    compute: impl FnOnce() -> String,
) -> HttpResponse {
    use crate::helpers::audit_cache::{cache_key, get, set};
    let key = cache_key(checker, body);
    if let Some(conn) = &state.redis {
        let mut conn = conn.clone();
        if let Some(hit) = get(&mut conn, &key).await {
            return json_response(hit, "HIT");
        }
        let json = compute();
        set(&mut conn, &key, &json, state.cache_ttl).await;
        return json_response(json, "MISS");
    }
    json_response(compute(), "BYPASS")
}

#[post("/compose")]
async fn compose(body: String, state: web::Data<AuditState>) -> impl Responder {
    cached_json(&state, "compose", body.as_bytes(), || {
        serde_json::to_string(&audit_compose(&body)).unwrap_or_default()
    })
    .await
}

#[post("/dockerfile")]
async fn dockerfile(body: String, state: web::Data<AuditState>) -> impl Responder {
    cached_json(&state, "dockerfile", body.as_bytes(), || {
        serde_json::to_string(&audit_dockerfile(&body)).unwrap_or_default()
    })
    .await
}

#[post("/exposure")]
async fn exposure(body: String, state: web::Data<AuditState>) -> impl Responder {
    cached_json(&state, "exposure", body.as_bytes(), || {
        serde_json::to_string(&audit_exposure(&body)).unwrap_or_default()
    })
    .await
}

#[post("/readiness")]
async fn readiness(body: String, state: web::Data<AuditState>) -> impl Responder {
    cached_json(&state, "readiness", body.as_bytes(), || {
        serde_json::to_string(&audit_readiness(&body)).unwrap_or_default()
    })
    .await
}

#[post("/cost")]
async fn cost(body: String, state: web::Data<AuditState>) -> impl Responder {
    cached_json(&state, "cost", body.as_bytes(), || {
        serde_json::to_string(&estimate_cost(&body, &DefaultPricing)).unwrap_or_default()
    })
    .await
}

#[derive(Debug, Deserialize)]
struct ImageQuery {
    image: String,
}

#[post("/image")]
async fn image(query: web::Json<ImageQuery>, state: web::Data<AuditState>) -> impl Responder {
    let image_ref = query.image.trim();
    let key = crate::helpers::audit_cache::cache_key("image", image_ref.as_bytes());

    // Cache hit avoids a Docker Hub round-trip (and a Trivy scan).
    if let Some(conn) = &state.redis {
        let mut conn = conn.clone();
        if let Some(hit) = crate::helpers::audit_cache::get(&mut conn, &key).await {
            return json_response(hit, "HIT");
        }
    }

    let meta = DockerHubMetadata;
    let json = match meta.fetch(image_ref).await {
        Ok(info) => {
            let vulns = if info.exists {
                scan_vulns(image_ref).await
            } else {
                vec![]
            };
            serde_json::to_string(&audit_image(&info, &vulns)).unwrap_or_default()
        }
        // Registry/transport failures are not cached.
        Err(err) => return HttpResponse::BadGateway().json(serde_json::json!({ "error": err })),
    };

    if let Some(conn) = &state.redis {
        let mut conn = conn.clone();
        crate::helpers::audit_cache::set(&mut conn, &key, &json, state.cache_ttl).await;
    }
    json_response(json, "MISS")
}

/// Best-effort CVE scan. Opt-in via `AUDIT_TRIVY_ENABLED=1` (image scanning is
/// heavy, so it stays off until deliberately enabled) and bounded by a timeout;
/// any failure degrades gracefully to "metadata only" rather than erroring.
async fn scan_vulns(image_ref: &str) -> Vec<Vulnerability> {
    if std::env::var("AUDIT_TRIVY_ENABLED").ok().as_deref() != Some("1") {
        return vec![];
    }
    let timeout = std::time::Duration::from_secs(120);
    match tokio::time::timeout(timeout, TrivyScanner.scan(image_ref)).await {
        Ok(Ok(vulns)) => vulns,
        Ok(Err(e)) => {
            tracing::warn!("trivy scan failed for {image_ref}: {e}");
            vec![]
        }
        Err(_) => {
            tracing::warn!("trivy scan timed out for {image_ref}");
            vec![]
        }
    }
}

/// [`VulnScanner`] backed by the `trivy` CLI (`trivy image … --format json`).
struct TrivyScanner;

#[async_trait]
impl VulnScanner for TrivyScanner {
    async fn scan(&self, image_ref: &str) -> Result<Vec<Vulnerability>, String> {
        let output = tokio::process::Command::new("trivy")
            .args([
                "image",
                "--quiet",
                "--scanners",
                "vuln",
                "--format",
                "json",
                image_ref,
            ])
            .output()
            .await
            .map_err(|e| format!("trivy not available: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "trivy exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(parse_trivy_report(&String::from_utf8_lossy(&output.stdout)))
    }
}

/// Docker Hub-backed [`ImageMetadata`] — only ever contacts hub.docker.com.
struct DockerHubMetadata;

#[async_trait]
impl ImageMetadata for DockerHubMetadata {
    async fn fetch(&self, image_ref: &str) -> Result<ImageInfo, String> {
        let has_digest = image_ref.contains('@');
        let name_and_tag = image_ref.split('@').next().unwrap_or(image_ref);
        let (name, tag) = match name_and_tag.rsplit_once(':') {
            Some((n, t)) => (n.to_string(), Some(t.to_string())),
            None => (name_and_tag.to_string(), None),
        };
        let repo = if name.contains('/') {
            name.clone()
        } else {
            format!("library/{name}")
        };
        let official = !name.contains('/') || name.starts_with("library/");
        let pinned = has_digest || tag.as_deref().map(|t| t != "latest").unwrap_or(false);

        let url = format!("https://hub.docker.com/v2/repositories/{repo}/");
        let resp = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("registry request failed: {e}"))?;
        let exists = resp.status().is_success();

        // Best-effort "last_updated" (days since last push).
        let last_updated_days = if exists {
            resp.json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("last_updated").and_then(|d| d.as_str()).map(String::from))
                .and_then(days_since_iso8601)
        } else {
            None
        };

        Ok(ImageInfo {
            reference: image_ref.to_string(),
            exists,
            official,
            pinned,
            last_updated_days,
        })
    }
}

/// Rough days-since for an ISO-8601 timestamp (no chrono dep needed here).
fn days_since_iso8601(ts: String) -> Option<u64> {
    let year: i64 = ts.get(0..4)?.parse().ok()?;
    let month: i64 = ts.get(5..7)?.parse().ok()?;
    let day: i64 = ts.get(8..10)?.parse().ok()?;
    // days from year 0 (approx, good enough for "stale > 1yr" bucketing)
    let to_days = |y: i64, m: i64, d: i64| y * 365 + (y / 4) + m * 30 + d;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64
        / 86_400
        + to_days(1970, 1, 1);
    let then = to_days(year, month, day);
    Some((now - then).max(0) as u64)
}
