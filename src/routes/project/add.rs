use crate::db;
use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{
    post, web,
    web::{Bytes, Data},
    Responder, Result,
};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::str::FromStr;
use std::str;

#[tracing::instrument(name = "Add project.")]
#[post("")]
pub async fn item(
    body: Bytes,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let form = forms::project::form::body_into_form(body.clone()).await?;
    let project_name = form.custom.custom_stack_code.clone();

    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes)
        .map_err(|err| JsonResponse::<forms::project::ProjectForm>::build().internal_server_error(err.to_string()))?;
    let request_json = Value::from_str(body_str).unwrap();
    tracing::debug!("Request json: {:?}", request_json);

    let body: Value = serde_json::to_value::<forms::project::ProjectForm>(form)
        .or(serde_json::to_value::<forms::project::ProjectForm>(forms::project::ProjectForm::default()))
        .unwrap();

    let project = models::Project::new(
        user.id.clone(),
        project_name,
        body,
        request_json
    );

    db::project::insert(pg_pool.get_ref(), project)
        .await
        .map(|project| JsonResponse::build().set_item(project).ok("Ok"))
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        })
}

