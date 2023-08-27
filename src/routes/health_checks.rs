use actix_web::{HttpRequest, HttpResponse};

pub async fn health_check(req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().finish()
}

