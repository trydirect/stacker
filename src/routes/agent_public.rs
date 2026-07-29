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

/// `POST /public/resolve_image` — `{ "reference": "redis:7-alpine" }` → the
/// `ResolvedImage` ground truth (exists, digest, size, architectures, tags,
/// grade). No authentication required.
#[post("/public/resolve_image")]
pub async fn resolve_image_public(body: web::Json<ResolveImageBody>) -> impl Responder {
    let reference = body.reference.trim();
    if reference.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({ "error": "`reference` is required" }));
    }
    match resolve_reference(reference).await {
        Ok(resolved) => HttpResponse::Ok().json(resolved),
        Err(err) => HttpResponse::BadGateway().json(serde_json::json!({ "error": err })),
    }
}
