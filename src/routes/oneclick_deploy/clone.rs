//! `POST /api/v1/deploy/clone` — the user-side of immutable deploy.
//!
//! Resolves a baked snapshot `image_id` for the requested stack/version, renders
//! per-user cloud-init (env + domain vhost), and clones a new Hetzner server
//! from the snapshot via `HetznerCloudConnector::create_server_from_image`.
//!
//! Protected (requires an authenticated user). Token is the TryDirect-managed
//! `HETZNER_TOKEN` (env), so users deploy on TryDirect's Hetzner account.

use std::collections::BTreeMap;
use std::sync::Arc;

use actix_web::web::Data;
use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::connectors::config::HetznerConfig;
use crate::connectors::hetzner::{
    HetznerCloudClient, HetznerCloudConnector, HetznerCreateServerRequest,
};
use crate::helpers::cloud_init::{render_user_data, BootConfig};
use crate::models::User;

#[derive(Debug, Deserialize)]
pub struct CloneRequest {
    pub stack: String,
    /// Optional. When omitted, the newest healthy snapshot for the stack is used.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_server_type")]
    pub server_type: String,
    pub domain: String,
    #[serde(default = "default_admin_email")]
    pub admin_email: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

fn default_provider() -> String {
    "hetzner".to_string()
}

fn default_region() -> String {
    "fsn1".to_string()
}

fn default_server_type() -> String {
    "cpx11".to_string()
}

fn default_admin_email() -> String {
    "admin@example.com".to_string()
}

#[derive(Debug, Serialize)]
pub struct CloneResponse {
    pub server_id: i64,
    pub public_ipv4: Option<String>,
    pub stack: String,
    pub provider: String,
}

#[post("/clone")]
pub async fn clone_server(
    user: web::ReqData<Arc<User>>,
    form: web::Json<CloneRequest>,
    pg_pool: Data<PgPool>,
) -> impl Responder {
    tracing::debug!(
        user_id = %user.id,
        stack = %form.stack,
        region = %form.region,
        "clone deploy requested"
    );

    // Resolve the baked snapshot image_id.
    let snapshot = match if let Some(version) = &form.version {
        crate::db::baked_snapshot::resolve(&pg_pool, &form.stack, version, &form.provider).await
    } else {
        crate::db::baked_snapshot::resolve_latest(&pg_pool, &form.stack, &form.provider).await
    } {
        Ok(snap) => snap,
        Err(err) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "snapshot registry error",
                "details": err.to_string(),
            }))
        }
    };

    let Some(snapshot) = snapshot else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "No baked snapshot found",
            "details": format!(
                "no healthy baked snapshot for stack '{}' (provider: {}) in the registry",
                form.stack, form.provider
            ),
        }));
    };

    let Some(image_id) = snapshot.clone_image_id() else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Baked snapshot not healthy",
            "details": format!(
                "snapshot for '{}' v{} exists but is marked unhealthy",
                form.stack,
                snapshot.version
            ),
        }));
    };

    // Render cloud-init with per-user env + domain. Secrets are expected to be
    // pre-resolved (by the user service) into `form.env`.
    let boot = BootConfig {
        domain: form.domain.clone(),
        admin_email: form.admin_email.clone(),
        env: form.env.clone(),
    };
    let user_data = render_user_data(&boot);

    // TryDirect-managed Hetzner credentials.
    let htz = HetznerConfig::from_env();
    let Some(token) = htz.token.as_deref().filter(|t| !t.trim().is_empty()) else {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Hetzner not configured",
            "details": "HETZNER_TOKEN is not set on the TryDirect stacker backend",
        }));
    };

    let client = match HetznerCloudClient::new(htz.base_url.clone()) {
        Ok(client) => client,
        Err(err) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Hetzner client init failed",
                "details": err.to_string(),
            }))
        }
    };

    let request = HetznerCreateServerRequest {
        name: format!("{}-{}", form.stack, snapshot.version),
        server_type: form.server_type.clone(),
        location: form.region.clone(),
        image_id,
        ssh_key_ids: Vec::new(),
        user_data: Some(user_data),
    };

    let provisioned = match client.create_server_from_image(token, request).await {
        Ok(server) => server,
        Err(err) => {
            tracing::error!(error = %err, "clone from snapshot failed");
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": "Hetzner clone failed",
                "details": err.to_string(),
            }));
        }
    };

    HttpResponse::Ok().json(CloneResponse {
        server_id: provisioned.id,
        public_ipv4: provisioned.public_ipv4,
        stack: form.stack.clone(),
        provider: form.provider.clone(),
    })
}
