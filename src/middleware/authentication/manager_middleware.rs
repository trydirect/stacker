use crate::middleware::authentication::*;
use actix_web::{error::ErrorBadRequest, Error, dev::{ServiceRequest, ServiceResponse, Service}};
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
        if let Some(mut guard) = self.service.try_lock() {
            guard.poll_ready(ctx)
        } else {
            // Another request is in-flight; signal pending instead of panicking
            Poll::Pending
        }
    }

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        async move {
            let _ = method::try_agent(&mut req).await?
            || method::try_oauth(&mut req).await?
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
