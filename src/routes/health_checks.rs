use actix_web::{get, HttpRequest, HttpResponse};

#[get("")]
pub async fn health_check(_req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().finish()
}
