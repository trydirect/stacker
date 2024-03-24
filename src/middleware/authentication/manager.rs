use crate::middleware::authentication::*;

use std::sync::Arc;
use std::future::{ready, Ready};
use futures::lock::Mutex;

use actix_web::{ 
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};

pub struct Manager {}

impl Manager {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S, B> Transform<S, ServiceRequest> for Manager
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = ManagerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ManagerMiddleware {
            service: Arc::new(Mutex::new(service)),
        }))
    }
}
