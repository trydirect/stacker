use crate::configuration::Settings;
use crate::connectors::user_service::UserServiceConnector;
use crate::{db, helpers};
use actix_web::{post, web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AgentLoginRequest {
    pub email: String,
    pub password: String,
}

impl std::fmt::Debug for AgentLoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoginRequest")
            .field("email", &self.email)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct DeploymentInfo {
    pub deployment_id: String,
    pub stack_name: String,
    pub status: String,
    pub created_at: Option<String>,
    pub server_ip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AgentLoginResponse {
    pub session_token: String,
    pub user_id: String,
    pub deployments: Vec<DeploymentInfo>,
}

/// POST /api/v1/agent/login
///
/// Proxy login for Status Panel agents. Authenticates the user against
/// the TryDirect OAuth server, then returns a session token and the
/// user's deployments so the agent can pick one to link to.
#[tracing::instrument(name = "Agent proxy login", skip_all)]
#[post("/login")]
pub async fn login_handler(
    payload: web::Json<AgentLoginRequest>,
    settings: web::Data<Settings>,
    api_pool: web::Data<sqlx::PgPool>,
    user_service: web::Data<Arc<dyn UserServiceConnector>>,
    _req: HttpRequest,
) -> Result<HttpResponse> {
    // 1. Authenticate user against TryDirect OAuth server.
    // /auth/login lives at the user-service root, not under /oauth_server/api/,
    // so we use normalize_user_service_base_url() which strips the full tail.
    let auth_base = crate::connectors::user_service::plan::normalize_user_service_base_url(
        &settings.auth_url,
    );
    let login_url = format!("{}/auth/login", auth_base);

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| {
            helpers::JsonResponse::<AgentLoginResponse>::build()
                .internal_server_error(format!("HTTP client error: {}", e))
        })?;

    let resp = http_client
        .post(&login_url)
        .form(&[
            ("email", payload.email.as_str()),
            ("password", payload.password.as_str()),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("OAuth request failed: {:?}", e);
            helpers::JsonResponse::<AgentLoginResponse>::build()
                .internal_server_error(format!("Authentication service unreachable: {}", e))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let _body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            status = %status,
            "Agent login authentication failed for {}",
            payload.email
        );
        return Err(helpers::JsonResponse::<AgentLoginResponse>::build()
            .forbidden(format!("Authentication failed ({})", status)));
    }

    // Parse the OAuth token response
    #[derive(Deserialize)]
    struct TokenResp {
        access_token: String,
    }

    let token_resp: TokenResp = resp.json().await.map_err(|e| {
        helpers::JsonResponse::<AgentLoginResponse>::build()
            .internal_server_error(format!("Invalid auth response: {}", e))
    })?;

    let access_token = token_resp.access_token;

    // 2. Fetch user profile using the access token
    let profile = user_service
        .get_user_profile(&access_token)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch user profile: {:?}", e);
            helpers::JsonResponse::<AgentLoginResponse>::build()
                .internal_server_error(format!("Failed to fetch user profile: {}", e))
        })?;

    // 3. Fetch user's deployments from Stacker DB
    let deployments = db::deployment::fetch_by_user(api_pool.get_ref(), &profile.email, 50)
        .await
        .map_err(|e| {
            helpers::JsonResponse::<AgentLoginResponse>::build()
                .internal_server_error(format!("Failed to fetch deployments: {}", e))
        })?;

    let deployment_infos: Vec<DeploymentInfo> = deployments
        .into_iter()
        .filter(|d| d.deleted != Some(true))
        .map(|d| {
            let stack_name = d
                .metadata
                .get("project_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Stack")
                .to_string();

            let server_ip = d
                .metadata
                .get("server_ip")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            DeploymentInfo {
                deployment_id: d.deployment_hash.clone(),
                stack_name,
                status: d.status.clone(),
                created_at: Some(d.created_at.format("%Y-%m-%d %H:%M").to_string()),
                server_ip,
            }
        })
        .collect();

    tracing::info!(
        email = %payload.email,
        deployments = deployment_infos.len(),
        "Agent login successful"
    );

    Ok(HttpResponse::Ok().json(AgentLoginResponse {
        session_token: access_token,
        user_id: profile.email,
        deployments: deployment_infos,
    }))
}

#[cfg(test)]
mod tests {
    use crate::connectors::user_service::plan::normalize_user_service_base_url;

    /// Regression test: the production `auth_url` points at the OAuth `/me`
    /// endpoint, but `/auth/login` lives at the user-service root (sibling
    /// Flask blueprint, not nested). The derivation must strip the full
    /// `/oauth_server/api/me` tail, not just `/me`.
    #[test]
    fn login_url_is_derived_from_me_endpoint_correctly() {
        let auth_url = "https://dev.try.direct/server/user/oauth_server/api/me";
        let base = normalize_user_service_base_url(auth_url);
        let login_url = format!("{}/auth/login", base);
        assert_eq!(
            login_url,
            "https://dev.try.direct/server/user/auth/login"
        );
    }

    #[test]
    fn login_url_is_idempotent_when_auth_url_already_points_at_login() {
        // If an operator misconfigures auth_url to already point at /auth/login,
        // we must not produce a doubled path like /auth/login/auth/login.
        let auth_url = "https://dev.try.direct/server/user/auth/login";
        let base = normalize_user_service_base_url(auth_url);
        let login_url = format!("{}/auth/login", base);
        assert_eq!(
            login_url,
            "https://dev.try.direct/server/user/auth/login"
        );
    }

    #[test]
    fn login_url_handles_trailing_slash_in_auth_url() {
        let auth_url = "https://dev.try.direct/server/user/oauth_server/api/me/";
        let base = normalize_user_service_base_url(auth_url);
        let login_url = format!("{}/auth/login", base);
        assert_eq!(
            login_url,
            "https://dev.try.direct/server/user/auth/login"
        );
    }
}
