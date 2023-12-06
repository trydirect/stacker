use crate::configuration::Settings;
use futures::future::{FutureExt};
use crate::forms::user::UserForm;
use actix_web::dev::ServiceRequest;
use actix_web::error::{ErrorInternalServerError, ErrorUnauthorized};
use actix_web::web::{self};
use actix_web::Error;
use actix_web::HttpMessage;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use crate::helpers::JsonResponse;

use crate::models::user::User; //todo

#[tracing::instrument(name = "Trydirect bearer guard.")]
pub async fn bearer_guard( req: ServiceRequest, credentials: BearerAuth) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let token = credentials.token();
    let user = fetch_user(settings.auth_url.as_str(), token).await;
    if let Err(err) = user {
        return Err((ErrorUnauthorized(JsonResponse::<i32>::build().set_msg(err).to_string()), req));
    }

    let user = user.unwrap();
    if req.extensions_mut().insert(user).is_some() {
        return Err((ErrorUnauthorized(JsonResponse::<i32>::build().set_msg("user already logged").to_string()), req));
    }

    Ok(req)
}

async fn fetch_user(auth_url: &str, token: &str) -> Result<User, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(auth_url)
        .bearer_auth(token)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| "no resp from auth server".to_string())?;

    if !resp.status().is_success() {
        return Err("401 Unauthorized".to_string());
    }

    resp
        .json::<UserForm>()
        .await
        .map_err(|err| "can't parse the response body".to_string())?
        .try_into()
}
