use actix_web::{post, web, Responder, Result};
use serde::Serialize;

#[derive(Serialize)]
struct DeployResponse {
    status: String,
}

//todo inject client through enpoint's inputs
#[tracing::instrument(name = "Test deploy.")]
#[post("/deploy")]
pub async fn handler() -> Result<impl Responder> {
    Ok(web::Json(DeployResponse {
        status: "success".to_string(),
    }))
}
