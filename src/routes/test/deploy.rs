use crate::models::Client;
use actix_web::{post, web, Responder, Result};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
struct DeployResponse {
    status: String,
    client: Arc<Client>,
}

//todo inject client through enpoint's inputs
#[tracing::instrument(name = "Test deploy.")]
#[post("/deploy")]
pub async fn handler(client: web::ReqData<Arc<Client>>) -> Result<impl Responder> {
    Ok(web::Json(DeployResponse {
        status: "success".to_string(),
        client: client.into_inner(),
    }))
}
