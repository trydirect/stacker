//! Public "audit checker" endpoints under `/api/audit/*`.
//!
//! Thin actix wrappers over the pure `td_audit` engines. Unauthenticated
//! (granted to `group_anonymous` in casbin); paste-based checkers take the raw
//! body as text. Image inspection fetches Docker Hub metadata only (no
//! arbitrary host — no SSRF surface here) and leaves CVE scanning to a future
//! `VulnScanner` (Trivy) integration.

use actix_web::{post, web, HttpResponse, Responder, Scope};
use async_trait::async_trait;
use serde::Deserialize;

use td_audit::compose::audit_compose;
use td_audit::cost::{estimate_cost, DefaultPricing};
use td_audit::dockerfile::audit_dockerfile;
use td_audit::exposure::audit_exposure;
use td_audit::image::{audit_image, ImageInfo, ImageMetadata};
use td_audit::readiness::audit_readiness;

/// Mountable scope: `App::new().service(routes::audit::scope())`.
pub fn scope() -> Scope {
    web::scope("/api/audit")
        .service(compose)
        .service(dockerfile)
        .service(exposure)
        .service(readiness)
        .service(cost)
        .service(image)
}

#[post("/compose")]
async fn compose(body: String) -> impl Responder {
    HttpResponse::Ok().json(audit_compose(&body))
}

#[post("/dockerfile")]
async fn dockerfile(body: String) -> impl Responder {
    HttpResponse::Ok().json(audit_dockerfile(&body))
}

#[post("/exposure")]
async fn exposure(body: String) -> impl Responder {
    HttpResponse::Ok().json(audit_exposure(&body))
}

#[post("/readiness")]
async fn readiness(body: String) -> impl Responder {
    HttpResponse::Ok().json(audit_readiness(&body))
}

#[post("/cost")]
async fn cost(body: String) -> impl Responder {
    HttpResponse::Ok().json(estimate_cost(&body, &DefaultPricing))
}

#[derive(Debug, Deserialize)]
struct ImageQuery {
    image: String,
}

#[post("/image")]
async fn image(query: web::Json<ImageQuery>) -> impl Responder {
    let meta = DockerHubMetadata;
    match meta.fetch(&query.image).await {
        // CVE scanning (Trivy) is a follow-up; grade on metadata for now.
        Ok(info) => HttpResponse::Ok().json(audit_image(&info, &[])),
        Err(err) => HttpResponse::BadGateway().json(serde_json::json!({ "error": err })),
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
