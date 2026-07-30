//! Public, unauthenticated agent endpoints served by the agent-gateway.
//!
//! `resolve_image` is read-only ground truth with no cost, so it is exposed
//! without auth — an agent (or anyone) can call it with zero signup, which is
//! the frictionless top of the funnel. Anonymous access is granted via a casbin
//! rule for `group_anonymous` (see the accompanying migration).

use actix_web::{post, web, HttpResponse, Responder};
use serde::Deserialize;

use crate::mcp::tools::resolve_image::resolve_reference;

#[derive(Debug, Deserialize)]
pub struct ResolveImageBody {
    pub reference: String,
}

#[derive(Debug, Deserialize)]
pub struct ResolveImageQuery {
    pub reference: Option<String>,
}

/// `POST /public/resolve_image` → the `ResolvedImage` ground truth (exists,
/// digest, size, architectures, tags, grade). No authentication required.
///
/// Lenient on input so agents don't trip on content-type: accepts a JSON body
/// `{"reference": "redis:7-alpine"}` regardless of the Content-Type header, or a
/// `?reference=redis:7-alpine` query parameter.
#[post("/public/resolve_image")]
pub async fn resolve_image_public(
    body: web::Bytes,
    query: web::Query<ResolveImageQuery>,
) -> impl Responder {
    // Prefer the JSON body (parsed content-type-agnostically); fall back to the
    // query param.
    let reference = serde_json::from_slice::<ResolveImageBody>(&body)
        .map(|b| b.reference)
        .ok()
        .or_else(|| query.reference.clone())
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty());

    let reference = match reference {
        Some(r) => r,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "provide an image reference as JSON body {\"reference\":\"…\"} or ?reference=…"
            }));
        }
    };

    match resolve_reference(&reference).await {
        Ok(resolved) => HttpResponse::Ok().json(resolved),
        Err(err) => HttpResponse::BadGateway().json(serde_json::json!({ "error": err })),
    }
}
