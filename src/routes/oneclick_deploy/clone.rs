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
use crate::connectors::user_service::UserServiceConnector;
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
    /// Present only for deployment_daily templates. The user service stores
    /// this so it can void on failure or pass to deploy-complete for capture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_id: Option<String>,
}

#[post("/clone")]
pub async fn clone_server(
    user: web::ReqData<Arc<User>>,
    form: web::Json<CloneRequest>,
    pg_pool: Data<PgPool>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
) -> impl Responder {
    tracing::debug!(
        user_id = %user.id,
        stack = %form.stack,
        region = %form.region,
        "clone deploy requested"
    );

    // Require a real user token up-front. The middleware resolves the user
    // from several auth methods (agent, jwt, hmac, ...) that don't carry a
    // user-service token; clone needs one for billing and service callbacks.
    // 401 here lets the frontend redirect the user to sign in.
    if user.access_token.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return HttpResponse::Unauthorized().json(json!({
            "error": "Unauthorized",
            "details": "User access token is missing",
        }));
    }

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

    // ── Deployment-daily billing: authorize before server creation ────────
    let mut authorization_id: Option<String> = None;
    match crate::db::marketplace::get_approved_by_slug(&pg_pool, &form.stack).await {
        Ok(Some(template)) => {
            tracing::info!(
                template_slug = %form.stack,
                billing_cycle = ?template.billing_cycle,
                daily_rate = ?template.daily_rate,
                "template found for billing check"
            );
            if template.billing_cycle.as_deref() == Some("deployment_daily") {
                // Resolve daily_rate: template override or server-type default
                let daily_rate = if let Some(rate) = template.daily_rate {
                    rate
                } else if let Ok(Some(cfg)) =
                    crate::db::server_type_daily_rate::fetch(&pg_pool, &form.server_type).await
                {
                    cfg.daily_rate
                } else {
                    0.87 // fallback default
                };
                let monthly_cap = template.monthly_cap.unwrap_or_else(|| daily_rate * 30.0);

                // Convert to minor units (cents)
                let amount_minor = (daily_rate * 100.0).round() as i64;
                let currency = template
                    .currency
                    .clone()
                    .unwrap_or_else(|| "USD".to_string());
                let idem_key = format!("oneclick-{}", deployment_hash);

                // Get user's access token for authorization
                let user_token = user.access_token.as_deref().unwrap_or("");
                if !user_token.is_empty() {
                    match user_service
                        .authorize_install_charge(
                            user_token,
                            &template.id,
                            amount_minor,
                            &currency,
                            &idem_key,
                        )
                        .await
                    {
                        Ok(handle) => {
                            let expires_at = handle
                                .expires_at
                                .as_deref()
                                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                .map(|dt| dt.with_timezone(&chrono::Utc));

                            match crate::db::marketplace_billing::insert_authorization(
                                &pg_pool,
                                crate::db::marketplace_billing::NewAuthorization {
                                    user_id: user.id.clone(),
                                    template_id: template.id,
                                    idempotency_key: idem_key,
                                    authorization_id: handle.authorization_id.clone(),
                                    amount_minor: handle.amount_minor,
                                    currency: handle.currency.clone(),
                                    expires_at,
                                    billing_cycle: Some("deployment_daily".to_string()),
                                    daily_rate: Some(daily_rate),
                                    monthly_cap: Some(monthly_cap),
                                },
                            )
                            .await
                            {
                                Ok(auth_row) => {
                                    crate::db::marketplace_billing::attach_deployment_hash(
                                        &pg_pool,
                                        auth_row.id,
                                        &deployment_hash,
                                    )
                                    .await
                                    .ok();
                                    authorization_id = Some(handle.authorization_id);
                                    tracing::info!(
                                        deployment_hash = %deployment_hash,
                                        daily_rate = daily_rate,
                                        monthly_cap = monthly_cap,
                                        "deployment_daily authorization created"
                                    );
                                }
                                Err(err) => {
                                    tracing::error!("Failed to store authorization: {}", err);
                                    let _ = user_service
                                        .void_install_charge(
                                            user_token,
                                            &handle.authorization_id,
                                            "db_write_failed",
                                        )
                                        .await;
                                    return HttpResponse::InternalServerError().json(json!({
                                        "error": "authorization storage failed",
                                        "details": err,
                                    }));
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!("authorize_install_charge failed: {:?}", err);
                            return HttpResponse::PaymentRequired().json(json!({
                                "error": "Payment authorization failed",
                                "details": format!("{:?}", err),
                            }));
                        }
                    }
                } else {
                    tracing::warn!(
                        "deployment_daily template but user has no access_token, skipping authorize"
                    );
                }
            } else {
                tracing::info!(
                    template_slug = %form.stack,
                    billing_cycle = ?template.billing_cycle,
                    "template is not deployment_daily, skipping billing"
                );
            }
        }
        Ok(None) => {
            tracing::warn!(
                template_slug = %form.stack,
                "stack not registered in stack_template; refusing to deploy"
            );
            return HttpResponse::NotFound().json(json!({
                "error": "Unknown stack",
                "details": format!(
                    "stack '{}' is not registered in the marketplace catalog",
                    form.stack
                ),
            }));
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to look up template for billing");
        }
    }

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
            &format!(
                "deploy-{}-{}",
                form.stack,
                &deployment_hash[deployment_hash.len() - 8..]
            ),
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
        name: format!(
            "{}-{}-{}",
            form.stack,
            snapshot.version,
            &deployment_hash[11..19]
        ),
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
        authorization_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;
    use actix_web::web;
    use actix_web::HttpMessage;

    fn test_user(token: Option<String>) -> Arc<User> {
        Arc::new(User {
            id: "test-user-1".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: "test@example.com".to_string(),
            role: "user".to_string(),
            email_confirmed: true,
            mfa_verified: true,
            access_token: token,
        })
    }

    fn valid_payload() -> serde_json::Value {
        serde_json::json!({
            "stack": "wordpress",
            "provider": "hetzner",
            "region": "fsn1",
            "server_type": "cpx11",
            "domain": "example.com",
            "admin_email": "admin@example.com",
            "env": {},
        })
    }

    async fn call_clone(user: Arc<User>) -> actix_web::http::StatusCode {
        let pg_pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/stacker_test")
            .expect("lazy pool");
        let user_service: Arc<dyn crate::connectors::user_service::UserServiceConnector> =
            Arc::new(crate::connectors::user_service::mock::MockUserServiceConnector);

        let app = test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pg_pool))
                .app_data(web::Data::new(user_service))
                .service(clone_server),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/clone")
            .set_json(valid_payload())
            .insert_header(("Authorization", "Bearer dummy-token"))
            .to_request();
        req.extensions_mut().insert(Arc::clone(&user));

        let resp = test::call_service(&app, req).await;
        resp.status()
    }

    #[actix_web::test]
    async fn clone_without_access_token_returns_401() {
        let status = call_clone(test_user(None)).await;
        assert_eq!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn clone_with_blank_access_token_returns_401() {
        let status = call_clone(test_user(Some("   ".to_string()))).await;
        assert_eq!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn clone_with_access_token_passes_auth_guard() {
        // The guard should pass; the handler then proceeds (and fails later
        // on snapshot/DB/network, not on auth).
        let status = call_clone(test_user(Some("valid-token".to_string()))).await;
        assert_ne!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
