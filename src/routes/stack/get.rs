use actix_web::{web, HttpResponse};
// use chrono::Utc;
use sqlx::PgPool;
// use uuid::Uuid;

pub async fn get(
    id: web::Path<String>,
    pool: web::Data<PgPool>,
) -> HttpResponse {
    let id = id.into_inner();
    tracing::info!("Get stack by id {:?}", id);

    match sqlx::query!(
        r#"
        SELECT id FROM user_stack
        WHERE id=$1
        "#,
        id.parse::<i32>().unwrap()
    )
    .fetch_one(pool.get_ref())
    .await
    {
        Ok(_) => {
            tracing::info!("Stack found by id {}", id);
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("Failed to execute query: {:?}", e);
            HttpResponse::NotFound().finish()
        }
    }
}
