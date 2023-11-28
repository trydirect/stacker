use crate::configuration::Settings;
use crate::forms::user::UserForm;
use actix_web::dev::ServiceRequest;
use actix_web::error::{ErrorInternalServerError, ErrorUnauthorized};
use actix_web::web::{self};
use actix_web::Error;
use actix_web::HttpMessage;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use reqwest::header::{ACCEPT, CONTENT_TYPE};

use crate::models::user::User;

#[tracing::instrument(name = "Trydirect bearer guard.")]
pub async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let client = reqwest::Client::new();
    let resp = client
        .get(&settings.auth_url)
        .bearer_auth(credentials.token())
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await;

    let resp = match resp {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            tracing::error!("Authentication service returned no success {:?}", resp);
            return Err((ErrorUnauthorized("401 Unauthorized"), req));
        }
        Err(err) => {
            tracing::error!("error from reqwest {:?}", err);
            return Err((ErrorInternalServerError(err.to_string()), req));
        }
    };

    let user_form: UserForm = match resp.json().await {
        Ok(user) => {
            tracing::info!("unpacked user {user:?}");
            user
        }
        Err(err) => {
            tracing::error!("can't parse the response body {:?}", err);
            return Err((ErrorUnauthorized(""), req));
        }
    };

    let user: User = match user_form.try_into() {
        Ok(user) => user,
        Err(err) => {
            tracing::error!("Could not create User from form data: {:?}", err);
            return Err((ErrorUnauthorized("Unauthorized"), req));
        }
    };
    let existent_user = req.extensions_mut().insert(user);
    if existent_user.is_some() {
        tracing::error!("already logged {existent_user:?}");
        return Err((ErrorInternalServerError(""), req));
    }
    //todo move request outside

    Ok(req)
}
