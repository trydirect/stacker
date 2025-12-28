use crate::configuration::Settings;
use crate::middleware::authentication::get_header;
use crate::models;
use actix_web::{dev::ServiceRequest, web, HttpMessage};
use std::sync::Arc;

#[tracing::instrument(name = "Authenticate with cookie")]
pub async fn try_cookie(req: &mut ServiceRequest) -> Result<bool, String> {
    // Get Cookie header
    let cookie_header = get_header::<String>(&req, "cookie")?;
    if cookie_header.is_none() {
        return Ok(false);
    }

    // Parse cookies to find access_token
    let cookies = cookie_header.unwrap();
    let token = cookies
        .split(';')
        .find_map(|cookie| {
            let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
            if parts.len() == 2 && parts[0] == "access_token" {
                Some(parts[1].to_string())
            } else {
                None
            }
        });

    if token.is_none() {
        return Ok(false);
    }

    tracing::debug!("Found access_token in cookies");

    // Use same OAuth validation as Bearer token
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let user = super::f_oauth::fetch_user(settings.auth_url.as_str(), &token.unwrap())
        .await
        .map_err(|err| format!("{err}"))?;

    // Control access using user role
    tracing::debug!("ACL check for role (cookie auth): {}", user.role.clone());
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
