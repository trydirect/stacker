//! Proxies the admin-ui notification bell to User Service's `/notifications/*`
//! endpoints.
//!
//! Found 2026-10-05: admin-ui logs in through PostgREST's `rpc/login`, which
//! mints its own `{role, email, exp}` JWT signed with Postgres' shared
//! secret - the same JWT this service's own `_admin` extractor verifies.
//! User Service's OAuth2 server has no idea this JWT exists, so
//! `@oauth_server.require_oauth("profile")` on its notification endpoints
//! always 401'd for admin-ui. User Service now also accepts an
//! internal-service-authenticated call (`X-Internal-Service-Key` +
//! `?admin_email=`), and this module is the only intended caller of that
//! path - the admin-ui browser never holds `INTERNAL_SERVICES_ACCESS_KEY`.
//!
//! Security note (audit of the User Service side, 2026-10-05, finding 1):
//! `admin_email` is NOT cryptographically bound to the internal key on the
//! receiving end, so whoever can set it can read/mark-read ANY admin's
//! notifications. The constraint that makes this safe is enforced HERE: the
//! value sent is always `admin.email`, resolved from the caller's own
//! verified JWT via the `_admin` extractor below - it is never read from the
//! incoming admin-ui request's query string or body. Do not change that.

use crate::models;
use actix_web::{get, patch, web, HttpResponse, Responder, Result};
use serde::Deserialize;
use std::sync::Arc;

fn user_service_base_url() -> Result<String, String> {
    std::env::var("URL_SERVER_USER")
        .or_else(|_| std::env::var("USER_SERVICE_URL"))
        .or_else(|_| std::env::var("USER_SERVICE_BASE_URL"))
        .map_err(|_| "USER_SERVICE_URL not configured".to_string())
}

fn internal_service_key() -> Result<String, String> {
    std::env::var("INTERNAL_SERVICES_ACCESS_KEY")
        .map_err(|_| "INTERNAL_SERVICES_ACCESS_KEY not configured".to_string())
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client")
}

/// Turns a reqwest response (or a local configuration/transport error) into
/// the actix response to hand back to admin-ui, preserving User Service's
/// own status code and JSON body where we got one.
async fn relay(result: Result<reqwest::Response, String>) -> impl Responder {
    let response = match result {
        Ok(r) => r,
        Err(message) => {
            tracing::error!("admin notification bell proxy failed: {}", message);
            return HttpResponse::ServiceUnavailable().json(
                serde_json::json!({"error": "Notification service temporarily unavailable"}),
            );
        }
    };

    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or_else(
        |_| serde_json::json!({"error": "Notification service returned a non-JSON response"}),
    );

    HttpResponse::build(
        actix_web::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
    )
    .json(body)
}

async fn forward_get(
    path: &str,
    admin_email: &str,
    extra_query: &[(&str, String)],
) -> Result<reqwest::Response, String> {
    let base_url = user_service_base_url()?;
    let key = internal_service_key()?;

    let mut query: Vec<(&str, String)> = vec![("admin_email", admin_email.to_string())];
    query.extend_from_slice(extra_query);

    http_client()
        .get(format!("{}{}", base_url.trim_end_matches('/'), path))
        .header("X-Internal-Service-Key", key)
        .query(&query)
        .send()
        .await
        .map_err(|e| format!("request to User Service failed: {}", e))
}

#[derive(Deserialize)]
pub struct ListNotificationsQuery {
    pub is_read: Option<String>,
    pub page: Option<String>,
    pub limit: Option<String>,
}

#[tracing::instrument(name = "Admin notification bell: list", skip_all)]
#[get("")]
pub async fn list_handler(
    admin: web::ReqData<Arc<models::User>>,
    query: web::Query<ListNotificationsQuery>,
) -> impl Responder {
    let mut extra: Vec<(&str, String)> = Vec::new();
    if let Some(v) = &query.is_read {
        extra.push(("is_read", v.clone()));
    }
    if let Some(v) = &query.page {
        extra.push(("page", v.clone()));
    }
    if let Some(v) = &query.limit {
        extra.push(("limit", v.clone()));
    }

    relay(forward_get("/notifications/", &admin.email, &extra).await).await
}

#[tracing::instrument(name = "Admin notification bell: unread count", skip_all)]
#[get("/unread-count")]
pub async fn unread_count_handler(admin: web::ReqData<Arc<models::User>>) -> impl Responder {
    relay(forward_get("/notifications/unread-count", &admin.email, &[]).await).await
}

#[derive(Deserialize)]
pub struct UpdateNotificationRequest {
    pub is_read: bool,
}

#[tracing::instrument(name = "Admin notification bell: mark read", skip_all)]
#[patch("/{id}")]
pub async fn update_handler(
    admin: web::ReqData<Arc<models::User>>,
    path: web::Path<(String,)>,
    body: web::Json<UpdateNotificationRequest>,
) -> impl Responder {
    let id = path.into_inner().0;

    let result = async {
        let base_url = user_service_base_url()?;
        let key = internal_service_key()?;

        http_client()
            .patch(format!(
                "{}/notifications/{}",
                base_url.trim_end_matches('/'),
                id
            ))
            .header("X-Internal-Service-Key", key)
            .query(&[("admin_email", admin.email.clone())])
            .json(&serde_json::json!({"is_read": body.is_read}))
            .send()
            .await
            .map_err(|e| format!("request to User Service failed: {}", e))
    }
    .await;

    relay(result).await
}
