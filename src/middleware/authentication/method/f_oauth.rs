use crate::middleware::authentication::get_header;
use actix_web::{web, dev::{ServiceRequest}, HttpMessage};
use crate::configuration::Settings;
use crate::models;
use crate::forms;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use std::sync::Arc;

fn try_extract_token(authentication: String) -> Result<String, String> {
    let mut authentication_parts = authentication.splitn(2, ' ');
    match authentication_parts.next() {
        Some("Bearer") => {}
        _ => return Err("Bearer missing scheme".to_string())
    }
    let token = authentication_parts.next();
    if token.is_none() {
        tracing::error!("Bearer token is missing");
        return Err("Authentication required".to_string());
    }

    Ok(token.unwrap().into())
}

#[tracing::instrument(name = "Authenticate with bearer token")]
pub async fn try_oauth(req: &mut ServiceRequest) -> Result<bool, String> {
    let authentication = get_header::<String>(&req, "authorization")?;
    if authentication.is_none() {
        return Ok(false);
    }

    let token = try_extract_token(authentication.unwrap())?; 
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let user = fetch_user(settings.auth_url.as_str(), &token)
        .await
        .map_err(|err| format!("{err}"))?;

    // control access using user role
    tracing::debug!("ACL check for role: {}", user.role.clone());
    let acl_vals = actix_casbin_auth::CasbinVals {
        subject: user.role.clone(),
        domain: None,
    };

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err("user already logged".to_string());
    }

    if req.extensions_mut().insert(acl_vals).is_some() {
        return Err("Something wrong with access control".to_string());
    }

    Ok(true)
}

async fn fetch_user(auth_url: &str, token: &str) -> Result<models::User, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(auth_url)
        .bearer_auth(token)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_err| "No response from OAuth server".to_string())?;

    if !resp.status().is_success() {
        return Err("401 Unauthorized".to_string());
    }

    resp
        .json::<forms::UserForm>()
        .await
        .map_err(|_err| "can't parse the response body".to_string())?
        .try_into()
}
