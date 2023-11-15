use crate::models::Client;
use actix_web::error::{ErrorForbidden, ErrorInternalServerError, ErrorNotFound, PayloadError};
use actix_web::web::BytesMut;
use actix_web::HttpMessage;
use futures::future::{FutureExt, LocalBoxFuture};
use futures::lock::Mutex;
use futures::task::{Context, Poll};
use futures::StreamExt;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::future::{ready, Ready};
use std::sync::Arc;
use tracing::Instrument;

use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorBadRequest,
    http::header::HeaderName,
    web, Error,
};
use sqlx::{Pool, Postgres};

pub struct Guard {}

impl Guard {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S, B> Transform<S, ServiceRequest> for Guard
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = GuardMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(GuardMiddleware {
            service: Arc::new(Mutex::new(service)),
        }))
    }
}

pub struct GuardMiddleware<S> {
    service: Arc<Mutex<S>>,
}

impl<S, B> Service<ServiceRequest> for GuardMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<ServiceResponse<B>, Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service
            .try_lock()
            .expect("GuardMiddleware was called allready")
            .poll_ready(ctx)
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        async move {
            let client_id = match req.headers().get(HeaderName::from_static("stacker-id")) {
                Some(client_id) => client_id,
                None => {
                    return Err(ErrorBadRequest("missing header stacker-id"));
                }
            };

            let client_id: &str = match client_id.to_str() {
                Ok(v) => v,
                Err(_) => {
                    return Err(ErrorBadRequest("header stacker-id is not valid"));
                }
            };
            let client_id: i32 = match client_id.parse() {
                Ok(v) => v,
                Err(_) => {
                    return Err(ErrorBadRequest("header stacker-id is not valid"));
                }
            };

            let hash = match req.headers().get(HeaderName::from_static("stacker-hash")) {
                Some(hash) => hash,
                None => {
                    return Err(ErrorBadRequest("missing header stacker-hash"));
                }
            };
            let hash: String = match hash.to_str() {
                Ok(v) => v.to_owned(),
                Err(_) => {
                    return Err(ErrorBadRequest("header stacker-hash is not valid"));
                }
            };

            let query_span = tracing::info_span!("Fetching the client by ID");
            let db_pool = req.app_data::<web::Data<Pool<Postgres>>>().unwrap();

            let mut client: Client = match sqlx::query_as!(
                Client,
                r#"
            SELECT
               id, user_id, secret
            FROM client c
            WHERE c.id = $1
            "#,
                client_id,
            )
            .fetch_one(db_pool.get_ref())
            .instrument(query_span)
            .await
            {
                Ok(client) if client.secret.is_some() => client,
                Ok(_client) => {
                    return Err(ErrorForbidden("client is not active"));
                }
                Err(sqlx::Error::RowNotFound) => {
                    return Err(ErrorNotFound("the client is not found"));
                }
                Err(e) => {
                    tracing::error!("Failed to execute fetch query: {:?}", e);

                    return Err(ErrorInternalServerError(""));
                }
            };

            //todo creates BytesMut with beforehand allocated memory
            let body = req
                .take_payload()
                .fold(BytesMut::new(), |mut body, chunk| {
                    let chunk = chunk.unwrap(); //todo process the potential error of unwrap
                    body.extend_from_slice(&chunk); //todo

                    ready(body)
                })
                .await;

            let mut mac =
                match Hmac::<Sha256>::new_from_slice(client.secret.as_ref().unwrap().as_bytes()) {
                    Ok(mac) => mac,
                    Err(err) => {
                        tracing::error!("error generating hmac {err:?}");

                        return Err(ErrorInternalServerError(""));
                    }
                };

            mac.update(body.as_ref());
            let computed_hash = format!("{:x}", mac.finalize().into_bytes());
            if hash != computed_hash {
                return Err(ErrorBadRequest("hash is wrong"));
            }

            let (_, mut payload) = actix_http::h1::Payload::create(true);
            payload.unread_data(body.into());
            req.set_payload(payload.into());

            match req.extensions_mut().insert(Arc::new(client)) {
                Some(_) => {
                    tracing::error!("client middleware already called once");
                    return Err(ErrorInternalServerError(""));
                }
                None => {}
            }

            let service = service.lock().await;
            service.call(req).await
        }
        .boxed_local()
    }
}
