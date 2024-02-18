use crate::middleware::authentication::*;

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
            let authorization = get_header::<String>(&req, "authorization")?;
            let client_id = get_header::<i32>(&req, "stacker-id")?;
            if authorization.is_some() {
                try_authorize_bearer(&mut req, authorization.unwrap()).await?; 
            } else if client_id.is_some() {
                try_authorize_id_hash(&mut req, client_id.unwrap()).await?;
            } else {
                let accesscontrol_vals = actix_casbin_auth::CasbinVals {
                    subject: "anonym".to_string(),
                    domain: None,
                };
                if req.extensions_mut().insert(accesscontrol_vals).is_some() {
                    return Err("sth wrong with access control".to_string());
                }
            }

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
