use crate::helpers::JsonResponse;
use crate::models::Client;
use actix_http::header::CONTENT_LENGTH;
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
use std::str::FromStr;
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
            let client_id: i32 = get_header(&req, "stacker-id").map_err(|m| {
                ErrorBadRequest(JsonResponse::<Client>::build().set_msg(m).to_string())
            })?;
            let hash: String = get_header(&req, "stacker-hash").map_err(|m| {
                ErrorBadRequest(JsonResponse::<Client>::build().set_msg(m).to_string())
            })?;

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
                    return Err(ErrorForbidden(
                        JsonResponse::<Client>::build()
                            .set_msg("client is not active")
                            .to_string(),
                    ));
                }
                Err(sqlx::Error::RowNotFound) => {
                    return Err(ErrorNotFound(
                        JsonResponse::<Client>::build()
                            .set_msg("the client is not found")
                            .to_string(),
                    ));
                }
                Err(e) => {
                    tracing::error!("Failed to execute fetch query: {:?}", e);

                    return Err(ErrorInternalServerError(
                        JsonResponse::<Client>::build().to_string(),
                    ));
                }
            };

            let content_length: usize = get_header(&req, CONTENT_LENGTH.as_str()).map_err(|m| {
                ErrorBadRequest(JsonResponse::<Client>::build().set_msg(m).to_string())
            })?;
            let mut bytes = BytesMut::with_capacity(content_length);
            let mut payload = req.take_payload();
            while let Some(chunk) = payload.next().await {
                bytes.extend_from_slice(&chunk?);
            }

            let mut mac =
                match Hmac::<Sha256>::new_from_slice(client.secret.as_ref().unwrap().as_bytes()) {
                    Ok(mac) => mac,
                    Err(err) => {
                        tracing::error!("error generating hmac {err:?}");

                        return Err(ErrorInternalServerError(
                            JsonResponse::<Client>::build().to_string(),
                        ));
                    }
                };

            mac.update(bytes.as_ref());
            let computed_hash = format!("{:x}", mac.finalize().into_bytes());
            if hash != computed_hash {
                return Err(ErrorBadRequest(
                    JsonResponse::<Client>::build()
                        .set_msg("hash is wrong")
                        .to_string(),
                ));
            }

            let (_, mut payload) = actix_http::h1::Payload::create(true);
            payload.unread_data(bytes.into());
            req.set_payload(payload.into());

            match req.extensions_mut().insert(Arc::new(client)) {
                Some(_) => {
                    tracing::error!("client middleware already called once");
                    return Err(ErrorInternalServerError(
                        JsonResponse::<Client>::build().to_string(),
                    ));
                }
                None => {}
            }

            let service = service.lock().await;
            service.call(req).await
        }
        .boxed_local()
    }
}

fn get_header<T>(req: &ServiceRequest, header_name: &'static str) -> Result<T, String>
where
    T: FromStr,
{
    let header_value = req
        .headers()
        .get(HeaderName::from_static(header_name))
        .ok_or(format!("header {header_name} not found"))?;

    let header_value: &str = header_value
        .to_str()
        .map_err(|_| format!("header {header_name} can't be converted to string"))?; //map_err
                                                                                     //
    header_value
        .parse::<T>()
        .map_err(|_| format!("header {header_name} has wrong type"))
}
