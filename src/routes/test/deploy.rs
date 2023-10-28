use actix_web::{post, web, Responder, Result};
use serde::Serialize;

#[derive(Serialize)]
struct DeployResponse {
    status: String,
}

#[tracing::instrument(name = "Add rating.")]
#[post("/deploy")]
pub async fn handler() -> Result<impl Responder> {
    Ok(DeployResponse {
        status: "success".to_string(),
    })
}
