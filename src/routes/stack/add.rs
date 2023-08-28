use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;


#[derive(serde::Deserialize)]
pub struct FormData {
    id: String,
    stack_json: String,
}

pub async fn add(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a new stack", %request_id,
        user_id=?form.user_id,
        stack_json=?form.stack
    );

    // using `enter` is an async function
    let _request_span_guard = request_span.enter(); // ->exit

    tracing::info!(
        "request_id {} Adding '{}' '{}' as a new stack",
        request_id,
        form.user_id,
        form.stack_json
    );

    let query_span = tracing::info_span!(
        "Saving new stack details into the database"
    );

    match sqlx::query!(
        r#"
        INSERT INTO user_stack (id, user_id, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        Uuid::new_v4(),
        form.user_id,
        stack_name,
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
