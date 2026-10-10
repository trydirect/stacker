#![allow(clippy::await_holding_lock)]

mod common;

use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tokio::sync::OnceCell;

// A fresh `PgPool` per test, bound to that test's own runtime. Sharing one
// pool across `#[tokio::test]` functions hands out connections belonging to a
// runtime that has already been dropped, and `acquire()` then blocks for the
// full 120s timeout and fails with `PoolTimedOut`. The server still starts
// once; only the pool is per test.
static APP_CONFIG: OnceCell<common::TestAppConfig> = OnceCell::const_new();

async fn app() -> common::TestApp {
    common::get_or_init_app_fresh(&APP_CONFIG)
        .await
        .expect("Failed to start test app")
}

fn create_admin_jwt(email: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let header = json!({"alg": "HS256", "typ": "JWT"});
    let payload = json!({
        "role": "admin_service",
        "email": email,
        "exp": (Utc::now() + Duration::minutes(30)).timestamp(),
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string());

    let signature = common::sign_test_jwt(&header_b64, &payload_b64);
    format!("{}.{}.{}", header_b64, payload_b64, signature)
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// The actual bug report this proxy exists to fix: admin-ui's JWT isn't a
/// real User Service OAuth token, so User Service's own endpoints 401 it
/// directly. This proxy exists specifically so admin-ui never has to hold
/// the internal key, and so User Service only ever sees the admin's own
/// resolved email - never a client-supplied one (security audit 2026-10-05,
/// finding 1).
#[tokio::test]
async fn unread_count_forwards_the_jwt_email_and_internal_key() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());

    Mock::given(method("GET"))
        .and(path("/notifications/unread-count"))
        .and(query_param("admin_email", "ops@test.com"))
        .and(header("X-Internal-Service-Key", common::TEST_INTERNAL_KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"count": 4})))
        .mount(&mock_user_service)
        .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/admin/notifications/unread-count",
            app.address
        ))
        .header(
            "Authorization",
            format!("Bearer {}", create_admin_jwt("ops@test.com")),
        )
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_eq!(StatusCode::OK, response.status());
    let body: serde_json::Value = response.json().await.expect("valid JSON");
    assert_eq!(4, body["count"]);
}

/// The specific security property the audit called out: a caller of this
/// proxy cannot choose WHOSE notifications they see. The admin_email sent
/// upstream must always be the caller's own, resolved from their verified
/// JWT - this handler has no code path that reads a client-supplied
/// admin_email at all, so there is nothing in the request that could smuggle
/// one in, but this test pins that at the HTTP boundary rather than trusting
/// the source read.
#[tokio::test]
async fn admin_email_sent_upstream_is_always_the_jwt_owners_not_attacker_chosen() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());

    // Only a mock expecting the genuine JWT email is mounted. If the proxy
    // ever forwarded a different (e.g. query-string-supplied) admin_email,
    // no mock would match and wiremock would return a 500 - this would fail
    // loudly, not silently pass.
    Mock::given(method("GET"))
        .and(path("/notifications/unread-count"))
        .and(query_param("admin_email", "real-admin@test.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"count": 0})))
        .mount(&mock_user_service)
        .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/admin/notifications/unread-count?admin_email=someone-else@test.com",
            app.address
        ))
        .header(
            "Authorization",
            format!("Bearer {}", create_admin_jwt("real-admin@test.com")),
        )
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn list_forwards_pagination_and_is_read_query_params() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());

    Mock::given(method("GET"))
        .and(path("/notifications/"))
        .and(query_param("admin_email", "ops@test.com"))
        .and(query_param("is_read", "false"))
        .and(query_param("page", "2"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [],
            "pagination": {"page": 2, "limit": 10, "total": 0, "pages": 0}
        })))
        .mount(&mock_user_service)
        .await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/admin/notifications?is_read=false&page=2&limit=10",
            app.address
        ))
        .header(
            "Authorization",
            format!("Bearer {}", create_admin_jwt("ops@test.com")),
        )
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn mark_read_forwards_the_patch_body_and_jwt_email() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    let mock_user_service = MockServer::start().await;
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", &mock_user_service.uri());
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", &mock_user_service.uri());

    Mock::given(method("PATCH"))
        .and(path("/notifications/n-123"))
        .and(query_param("admin_email", "ops@test.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "n-123", "is_read": true
        })))
        .mount(&mock_user_service)
        .await;

    let response = reqwest::Client::new()
        .patch(format!("{}/api/admin/notifications/n-123", app.address))
        .header(
            "Authorization",
            format!("Bearer {}", create_admin_jwt("ops@test.com")),
        )
        .json(&json!({"is_read": true}))
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_eq!(StatusCode::OK, response.status());
}

#[tokio::test]
async fn unread_count_without_a_token_is_rejected() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/admin/notifications/unread-count",
            app.address
        ))
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_ne!(StatusCode::OK, response.status());
}

/// Upstream failure (User Service down, or the proxy's own env not
/// configured) must surface as a real error, not a silent empty 200 that
/// would make the bell quietly stop updating with no signal to debug from.
#[tokio::test]
async fn unread_count_surfaces_upstream_failure_instead_of_a_fake_200() {
    let _env_lock = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let app = app().await;
    // Point at a URL nothing is listening on.
    let _url_server_user = EnvGuard::set("URL_SERVER_USER", "http://127.0.0.1:1");
    let _user_service_url = EnvGuard::set("USER_SERVICE_URL", "http://127.0.0.1:1");

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/admin/notifications/unread-count",
            app.address
        ))
        .header(
            "Authorization",
            format!("Bearer {}", create_admin_jwt("ops@test.com")),
        )
        .send()
        .await
        .expect("Failed to call the proxy");

    assert_ne!(StatusCode::OK, response.status());
}
