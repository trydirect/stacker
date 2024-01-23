use crate::{models, configuration::Settings, forms::user::UserForm, helpers::JsonResponse};
use actix_web::{web, dev::ServiceRequest, Error, HttpMessage};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use futures::future::{FutureExt};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use std::sync::Arc;

#[tracing::instrument(name = "TryDirect bearer guard.")]
pub async fn bearer_guard( req: ServiceRequest, credentials: BearerAuth) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let token = credentials.token();
    let user = match fetch_user(settings.auth_url.as_str(), token).await {
        Ok(user) => user,
        Err(err) => {
            return Err((JsonResponse::<i32>::build().unauthorized(err), req));
        }
    };

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err((JsonResponse::<i32>::build().unauthorized("user already logged"), req));
    }

    let vals = actix_casbin_auth::CasbinVals { //todo
        subject: String::from("alice"),
        domain: Some("/pen/1".to_string()),
    };
    let result = req.extensions_mut().insert(vals); //todo
    tracing::error!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA {}", result.is_some()); //todo

    Ok(req)
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
        .json::<UserForm>()
        .await
        .map_err(|_err| "can't parse the response body".to_string())?
        .try_into()
}
