use actix_web::{get, Responder, Result};
use crate::helpers::JsonResponse;

#[tracing::instrument(name = "Test casbin.")]
#[get("")]
pub async fn handler() -> Result<impl Responder> {
    Ok(JsonResponse::<i32>::build().ok("success"))
}
