use actix_web::{web::{Data, Bytes, Json}, HttpResponse, HttpRequest, FromRequest};
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use chrono::Utc;
use crate::models::stack::FormData;
use crate::startup::AppState;


pub async fn add(req: HttpRequest, app_state: Data<AppState>, pool:
Data<PgPool>, body: Bytes) -> HttpResponse {
    let content_type = req.headers().get("content-type");
    println!("=================== Request Content-Type: {:?}", content_type);
    println!("request: {:?}", body);
    // println!("app: {:?}", body);
    tracing::info!("we are here");
    match Json::<FormData>::extract(&req).await {
        Ok(form) => println!("Hello from {:?}!", form),
        Err(err) => println!("error={:?}", err),
    };

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

    HttpResponse::Ok().finish()
}
