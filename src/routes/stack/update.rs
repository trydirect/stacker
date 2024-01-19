use chrono::Utc;
use actix_web::{
    web,
    web::{Data},
    Responder, Result,
};
use crate::forms::stack::StackForm;
use crate::helpers::JsonResponse;
use crate::models::user::User;
use actix_web::post;
use serde_json::Value;
use sqlx::PgPool;
use serde_valid::Validate;
use tracing::Instrument;
use uuid::Uuid;
use crate::models;
use std::sync::Arc;


#[tracing::instrument(name = "Update stack.")]
#[post("/{id}")]
pub async fn update(
    path: web::Path<(i32,)>,
    form: web::Json<StackForm>,
    user: web::ReqData<Arc<User>>,
    pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL

    let (id,) = path.into_inner();
    let query_span = tracing::info_span!("Check existence by id.");
    match sqlx::query_as!(
        models::Stack,
        r"SELECT * FROM user_stack WHERE id = $1",
        id
    )
        .fetch_one(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(stack) => {
            tracing::info!("Found record: {:?}", stack.id);
        }
        Err(e) => {
            tracing::error!("Failed to fetch record: {:?}, error: {:?}", id, e);
            return Err(JsonResponse::<models::Stack>::build()
                .not_found(format!("Object not found {}", id)));
        }
    };

    let stack_name = form.custom.custom_stack_code.clone();
    tracing::debug!("form data: {:?}", form);
    let user_id = user.id.clone();
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Validating a stack", %request_id,
        commonDomain=?&form.custom.project_name,
        region=?&form.region,
        domainList=?&form.domain_list
    );
    let _request_span_guard = request_span.enter(); // ->exit

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err();
        tracing::debug!("Invalid data received {:?}", &errors.to_string());

        return Err(JsonResponse::<models::Stack>::build().bad_request(errors.to_string()));
    }

    tracing::info!("request_id {} Updating '{}' '{}'",
        request_id,
        form.custom.project_name,
        form.region
    );
    let query_span = tracing::info_span!("Update stack details in db.");

    let form_inner = form.into_inner();

    if !form_inner.is_readable_docker_image().await.is_ok() {

        return Err(JsonResponse::<models::Stack>::build().bad_request("Can not access docker image"));
    }

    let body: Value = match serde_json::to_value::<StackForm>(form_inner) {
        Ok(body) => body,
        Err(err) => {
            tracing::error!("Request_id {} error unwrap body {:?}", request_id, err);
            serde_json::to_value::<StackForm>(StackForm::default()).unwrap()
        }
    };

    match sqlx::query!(
        r#"
        UPDATE user_stack
        SET stack_id=$2, user_id=$3, name=$4, body=$5, created_at=$6, updated_at=$7
        WHERE id=$1
        "#,
        id,
        Uuid::new_v4(),
        user_id,
        stack_name,
        body,
        Utc::now(),
        Utc::now(),
    )
        .execute(pool.get_ref())
        .instrument(query_span)
        .await
    {
        Ok(record) => {
            tracing::info!(
                "req_id: {} stack details have been saved to database",
                request_id
            );
            return Ok(JsonResponse::<models::Stack>::build().set_id(id).ok("OK"));
        }
        Err(e) => {
            tracing::error!("req_id: {} Failed to execute query: {:?}", request_id, e);
            return Err(JsonResponse::<models::Stack>::build().bad_request("Internal Server Error"));
        }
    };
}
