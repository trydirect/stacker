use actix_web::{web::{Data, Bytes, Json}, HttpResponse, HttpRequest, Responder, Result};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use chrono::Utc;
use crate::forms::stack::StackForm;
use crate::startup::AppState;
use std::str;
use crate::models::Stack;
use crate::utils::json::JsonResponse;


// pub async fn add(req: HttpRequest, app_state: Data<AppState>, pool:
pub async fn add(body: Bytes, app_state: Data<AppState>, pool: Data<PgPool>) -> Result<impl Responder>  {
    // let content_type = req.headers().get("content-type");
    // println!("Request Content-Type: {:?}", content_type);

    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes).unwrap();
    // method 1 let app_state: AppState = serde_json::from_str(body_str).unwrap();
    // method 2 let app_state = serde_json::from_str::<AppState>(body_str).unwrap();
    let form = serde_json::from_str::<StackForm>(body_str).unwrap();
    // println!("app: {:?}", form);

    let user_id = app_state.user_id;
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a new stack", %request_id,
        commonDomain=?&form.common_domain,
        region=?&form.region,
        domainList=?&form.domain_list
    );
    // using `enter` is an async function
    let _request_span_guard = request_span.enter(); // ->exit

    tracing::info!(
        "request_id {} Adding '{}' '{}' as a new stack",
        request_id,
        form.common_domain,
        form.region
    );

    let query_span = tracing::info_span!(
        "Saving new stack details into the database"
    );

    let stack = Stack {
        id: 0_i32,                 // internal stack id
        stack_id: Uuid::new_v4(),  // public uuid of the stack
        // user_id: Uuid::from_u128(user_id as u128),
        user_id: user_id,          //
        name: form.custom.custom_stack_code.clone(),
        body: serde_json::to_value::<StackForm>(form).unwrap_or_default(),
        // body: body_str.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now()
    };

    println!("stack object {:?}", stack);
    return match sqlx::query!(
        r#"
        INSERT INTO user_stack (id, stack_id, user_id, name, body, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id;
        "#,
        0_i32,
        stack.stack_id,
        stack.user_id,
        stack.name,
        // sqlx::types::Json(stack.body),
        stack.body,
        stack.created_at,
        stack.updated_at
    )
        .fetch_one(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(record) => {
            tracing::info!(
                "req_id: {} New stack details have been saved to database",
                request_id
            );
            Ok(Json(JsonResponse {
                status: "OK".to_string(),
                code: 200,
                message: format!("Object saved"),
                id: Some(record.id)
            }))
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
            Ok(Json(JsonResponse {
                status: "Error".to_string(),
                code: 400,
                message: e.to_string(),
                id: None
            }))
        }
    }

}
