use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use std::sync::Arc;
use crate::helpers::JsonResponse;

#[tracing::instrument(name = "Test deploy.")]
#[post("/deploy")]
pub async fn handler(client: web::ReqData<Arc<Client>>) -> Result<impl Responder> {
    Ok(JsonResponse::build().set_item(client.into_inner()).ok("success"))
}
