// use std::io::Read;
use actix_web::{web::{Data, Bytes, Json}, HttpResponse, HttpRequest, Responder, Result};
// use actix_web::error::{Error, JsonPayloadError, PayloadError};
// use sqlx::PgPool;
// use tracing::Instrument;
// use uuid::Uuid;
// use chrono::Utc;
use crate::forms::stack::StackForm;
// use crate::startup::AppState;
use std::str;
// use actix_web::web::Form;


// pub async fn add(req: HttpRequest, app_state: Data<AppState>, pool:
pub async fn add(body: Bytes) -> Result<impl Responder>  {
    // None::<i32>.expect("my error");
    // return Err(JsonPayloadError::Payload(PayloadError::Overflow).into());
    // let content_type = req.headers().get("content-type");
    // println!("=================== Request Content-Type: {:?}", content_type);

    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes).unwrap();
    // method 1
    // let app_state: AppState = serde_json::from_str(body_str).unwrap();
    // method 2
    // let app_state = serde_json::from_str::<AppState>(body_str).unwrap();
    // println!("request: {:?}", app_state);

    let stack = serde_json::from_str::<StackForm>(body_str).unwrap();
    println!("app: {:?}", stack);
    // println!("user_id: {:?}", data.user_id);
    // tracing::info!("we are here");
    // match Json::<StackForm>::extract(&req).await {
    //     Ok(form) => println!("Hello from {:?}!", form),
    //     Err(err) => println!("error={:?}", err),
    // };

    // let user_id = app_state.user_id;
    // let request_id = Uuid::new_v4();
    // let request_span = tracing::info_span!(
    //     "Validating a new stack", %request_id,
    //     commonDomain=?form.common_domain,
    //     region=?form.region,
    //     domainList=?form.domain_list
    // );
    //
    // // using `enter` is an async function
    // let _request_span_guard = request_span.enter(); // ->exit
    //
    // tracing::info!(
    //     "request_id {} Adding '{}' '{}' as a new stack",
    //     request_id,
    //     form.common_domain,
    //     form.region
    // );
    //
    // let query_span = tracing::info_span!(
    //     "Saving new stack details into the database"
    // );
    //
    // // match sqlx::query!(
    // //     r#"
    // //     INSERT INTO user_stack (id, user_id, name, created_at, updated_at)
    // //     VALUES ($1, $2, $3, $4, $5)
    // //     "#,
    // //     0_i32,
    // //     user_id,
    // //     form.common_domain,
    // //     Utc::now(),
    // //     Utc::now()
    // // )
    // // .execute(pool.get_ref())
    // // .instrument(query_span)
    // // .await
    // // {
    // //     Ok(_) => {
    // //         tracing::info!(
    // //             "req_id: {} New stack details have been saved to database",
    // //             request_id
    // //         );
    // //         HttpResponse::Ok().finish()
    // //     }
    // //     Err(e) => {
    // //         tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
    // //         HttpResponse::InternalServerError().finish()
    // //     }
    // // }

    // HttpResponse::Ok().finish()
    Ok(Json(stack))
    // Ok(HttpResponse::Ok().finish())
}
