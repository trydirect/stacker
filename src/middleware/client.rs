use crate::models::Client;
use actix_web::error::{ErrorForbidden, ErrorInternalServerError, ErrorNotFound};
use actix_web::HttpMessage;
use futures::future::{FutureExt, LocalBoxFuture};
use futures::lock::Mutex;
use futures::task::{Context, Poll};
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

    fn call(&self, req: ServiceRequest) -> Self::Future {
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
            let hash: &str = match hash.to_str() {
                Ok(v) => v,
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

            match req.extensions_mut().insert(Arc::new(client)) {
                Some(_) => {
                    tracing::error!("client middleware already called once");
                    return Err(ErrorInternalServerError(""));
                }
                None => {}
            }

            //todo compute hash of the request
            //todo compare the has of the request

            let service = service.lock().await;
            service.call(req).await
        }
        .boxed_local()
    }
}
