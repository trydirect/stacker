use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use serde::Serialize;
use std::sync::Arc;
use crate::helpers::JsonResponse;

#[derive(Serialize)]
struct DeployResponse {
    status: String,
    client: Arc<Client>,
}

#[tracing::instrument(name = "Test deploy.")]
#[post("/deploy")]
pub async fn handler(client: web::ReqData<Arc<Client>>) -> Result<impl Responder> {
    JsonResponse::build().set_item(client.into_inner()).ok("success")
}
