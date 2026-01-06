use actix_web::{get, web, HttpResponse};
use crate::health::{HealthChecker, HealthMetrics};
use std::sync::Arc;

#[get("")]
pub async fn health_check(
    checker: web::Data<Arc<HealthChecker>>,
) -> HttpResponse {
    let health_response = checker.check_all().await;
    
    if health_response.is_healthy() {
        HttpResponse::Ok().json(health_response)
    } else {
        HttpResponse::ServiceUnavailable().json(health_response)
    }
}

#[get("/metrics")]
pub async fn health_metrics(
    metrics: web::Data<Arc<HealthMetrics>>,
) -> HttpResponse {
    let stats = metrics.get_all_stats().await;
    HttpResponse::Ok().json(stats)
}
