use crate::configuration::Settings;
use actix_web::dev::ServiceRequest;
use actix_web::web::{self};
use actix_web::Error;
use actix_web_httpauth::extractors::bearer::BearerAuth;
use std::sync::Arc;

#[tracing::instrument(name = "Client bearer guard.")]
pub async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<web::Data<Arc<Settings>>>().unwrap();
    //todo get client id from headers
    //todo get hash from headers
    //todo get body
    //todo get client from db by id
    //todo check that client is enabled
    //todo compute hash of the body based on the secret
    //todo if equal inject client
    //todo if not equal 401

    /*
    let resp = match resp {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            tracing::error!("Authentication service returned no success {:?}", resp);
            // tracing::debug!("{:?}", resp.text().await.unwrap());
            return Err((ErrorUnauthorized("401 Unauthorized"), req));
        }
        Err(err) => {
            tracing::error!("error from reqwest {:?}", err);
            return Err((ErrorInternalServerError(err.to_string()), req));
        }
    };
    */

    Ok(req)
}
