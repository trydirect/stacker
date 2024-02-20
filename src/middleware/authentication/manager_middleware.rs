use crate::middleware::authentication::*;
use actix_web::{error::ErrorBadRequest, HttpMessage, Error, dev::{ServiceRequest, ServiceResponse, Service}};
use crate::helpers::JsonResponse;
use futures::{task::{Poll, Context}, future::{FutureExt, LocalBoxFuture}, lock::Mutex};
use crate::models;
use std::sync::Arc;

pub struct ManagerMiddleware<S> {
    pub service: Arc<Mutex<S>>,
}

impl<S, B> Service<ServiceRequest> for ManagerMiddleware<S>
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
            .expect("Authentication ManagerMiddleware was called allready")
            .poll_ready(ctx)
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        async move {
            method::try_oauth(&mut req).await?
            || method::try_hmac(&mut req).await?
            || method::anonym(&mut req)?;

            Ok(req)
        }
        .then(|req: Result<ServiceRequest, String>| async move {
            match req {
                Ok(req) => {
                    let service = service.lock().await;
                    service.call(req).await
                }
                Err(msg) => Err(ErrorBadRequest(
                    JsonResponse::<models::Client>::build().set_msg(msg).to_string(),
                )),
            }
        })
        .boxed_local()
    }
}
