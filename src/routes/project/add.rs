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
use serde_valid::Validate;

#[tracing::instrument(name = "Add project.")]
#[post("")]
pub async fn item(
    web::Json(request_json): web::Json<serde_json::Value>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let form: forms::project::ProjectForm= serde_json::from_value(request_json.clone())
        .map_err(JsonResponse::err_bad_request)?;
    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err();
        return Err(JsonResponse::err_bad_request(errors));
    }

    let project_name = form.custom.custom_stack_code.clone();
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
