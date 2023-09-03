use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use chrono::Utc;


#[derive(serde::Deserialize)]
pub struct FormData {
    commonDomain: String,
    region: String,
    domainList: String,
    user_id: i32
}

pub async fn add(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a new stack", %request_id,
        commonDomain=?form.commonDomain,
        region=?form.region,
        domainList=?form.domainList
    );

    // using `enter` is an async function
    let _request_span_guard = request_span.enter(); // ->exit

    tracing::info!(
        "request_id {} Adding '{}' '{}' as a new stack",
        request_id,
        form.commonDomain,
        form.region
    );

    let query_span = tracing::info_span!(
        "Saving new stack details into the database"
    );

    match sqlx::query!(
        r#"
        INSERT INTO user_stack (id, user_id, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        0_i32,
        form.user_id,
        form.commonDomain,
        Utc::now(),
        Utc::now()
    )
    .execute(pool.get_ref())
    .instrument(query_span)
    .await
    {
        Ok(_) => {
            tracing::info!(
                "req_id: {} New stack details have been saved to database",
                request_id
            );
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
