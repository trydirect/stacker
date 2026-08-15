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
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::connectors::config::HetznerConfig;
use crate::connectors::hetzner::{
    HetznerCloudClient, HetznerCloudConnector, HetznerCreateServerRequest,
};
use crate::helpers::cloud_init::{render_user_data, BootConfig};
use crate::helpers::VaultClient;
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
    pub deployment_hash: String,
    /// SSH private key (PEM) for the deploy key injected into the cloned server.
    /// The user service must pass this to the install service for Ansible access.
    pub ssh_private_key: String,
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

    // Generate deployment_hash and persist Project + Deployment records.
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let hex = &deployment_hash[deployment_hash.len() - 8..];
    let project_name = format!("oneclick-{}-{}", form.stack, hex);

    let project = match crate::db::project::insert(
        &pg_pool,
        crate::models::Project::new(
            user.id.clone(),
            project_name,
            json!({"source": "oneclick_clone", "stack": form.stack}),
            json!({}),
        ),
    )
    .await
    {
        Ok(p) => p,
        Err(err) => {
            tracing::error!(error = %err, "failed to create project for clone deploy");
            return HttpResponse::InternalServerError().json(json!({
                "error": "project creation failed",
                "details": err,
            }));
        }
    };

    let mut deployment = crate::models::Deployment::new(
        project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        "in_progress".to_string(),
        "runc".to_string(),
        json!({
            "source": "oneclick_clone",
            "stack": form.stack,
            "domain": form.domain,
            "provider": form.provider,
            "region": form.region,
        }),
    );
    deployment = match crate::db::deployment::insert(&pg_pool, deployment).await {
        Ok(d) => d,
        Err(err) => {
            tracing::error!(error = %err, "failed to create deployment for clone");
            return HttpResponse::InternalServerError().json(json!({
                "error": "deployment creation failed",
                "details": err,
            }));
        }
    };
    tracing::info!(
        deployment_id = deployment.id,
        deployment_hash = %deployment_hash,
        project_id = project.id,
        "clone deployment records created"
    );

    // Generate a per-deploy SSH keypair so Ansible can reach the server post-boot.
    let (public_key, private_key) = match VaultClient::generate_ssh_keypair() {
        Ok(pair) => pair,
        Err(err) => {
            tracing::error!(error = %err, "failed to generate SSH keypair");
            return HttpResponse::InternalServerError().json(json!({
                "error": "SSH key generation failed",
                "details": err,
            }));
        }
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

    let mut ssh_key_ids: Vec<i64> = Vec::new();
    match client
        .add_ssh_key(
            token,
            &format!("deploy-{}-{}", form.stack, &deployment_hash[deployment_hash.len() - 8..]),
            &public_key,
        )
        .await
    {
        Ok(ssh_key) => {
            ssh_key_ids.push(ssh_key.id);
        }
        Err(err) => {
            // Non-fatal: the server will be created without the key.  The user
            // can still add it manually, but post-deploy Ansible will fail.
            tracing::warn!(error = %err, "failed to register SSH key on Hetzner — post-deploy setup may fail");
        }
    }

    let request = HetznerCreateServerRequest {
        name: format!("{}-{}", form.stack, snapshot.version),
        server_type: form.server_type.clone(),
        location: form.region.clone(),
        image_id,
        ssh_key_ids,
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
        deployment_hash,
        ssh_private_key: private_key,
    })
}
