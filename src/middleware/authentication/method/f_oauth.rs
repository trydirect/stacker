use crate::middleware::authentication::get_header;
use actix_web::{web, dev::{ServiceRequest}, HttpMessage};
use crate::configuration::Settings;
use crate::models;
use crate::forms;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use std::sync::Arc;

#[tracing::instrument(name = "try authorize. Authorization header")]
pub async fn try_oauth(req: &mut ServiceRequest) -> Result<bool, String> {
    let authentication = get_header::<String>(&req, "authorization")?; //todo
    if authentication.is_none() {
        return Ok(false);
    }

    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let token = "abc"; //todo
    let user = match fetch_user(settings.auth_url.as_str(), token).await {
        Ok(user) => user,
        Err(err) => {
            return Err(format!("{}", err));
        }
    }; //todo . process the err

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err("user already logged".to_string());
    }

    let accesscontrol_vals = actix_casbin_auth::CasbinVals {
        subject: String::from("alice"), //todo username or anonymous
        domain: None,
    };
    if req.extensions_mut().insert(accesscontrol_vals).is_some() {
        return Err("sth wrong with access control".to_string());
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
        .map_err(|_err| "no resp from auth server".to_string())?;

    if !resp.status().is_success() {
        return Err("401 Unauthorized".to_string());
    }

    resp
        .json::<forms::UserForm>()
        .await
        .map_err(|_err| "can't parse the response body".to_string())?
        .try_into()
}
