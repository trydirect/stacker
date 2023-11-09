use std::future::{ready, Ready};

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::ErrorBadRequest;
use actix_web::http::header::HeaderName;
use actix_web::Error;
use futures_util::future::LocalBoxFuture;

pub struct Guard {}

impl Guard {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S, B> Transform<S, ServiceRequest> for Guard
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = GuardMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(GuardMiddleware { service }))
    }
}

pub struct GuardMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for GuardMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        println!("Hi from start. You requested: {}", req.path()); //todo remove it
                                                                  //let fut = self.service.call(req);
        let client_id = match req.headers().get(HeaderName::from_static("stacker-id")) {
            Some(client_id) => client_id,
            None => {
                return Box::pin(async { Err(ErrorBadRequest("missing header stacker-id")) });
            }
        };

        //todo retrieve db
        //todo check the client

        Box::pin(self.service.call(req))
    }
}

/*
use crate::configuration::Settings;
use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorBadRequest;
use actix_web::http::header::HeaderName;
use actix_web::web::{self};
use actix_web::Error;
use std::sync::Arc;

#[tracing::instrument(name = "Client bearer guard.")]
pub async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    tracing::info!("try to get client_id");
    /*
    let client_id = match req.headers().get(HeaderName::from_static("stacker-id")) {
        Some(client_id) => client_id,
        None => {
            return Err((ErrorBadRequest("missing header stacker-id"), req));
        }
    };
    tracing::info!("client_id={client_id:?}");
    */
    //let settings = req.app_data::<web::Data<Arc<Settings>>>().unwrap();
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
*/
