use crate::db;
use crate::forms::project::ProjectForm;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "Add project.")]
#[post("")]
pub async fn item(
    web::Json(request_json): web::Json<serde_json::Value>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let form: ProjectForm = serde_json::from_value(request_json.clone())
        .map_err(|err| JsonResponse::bad_request(err.to_string()))?;
    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err();
        return Err(JsonResponse::bad_request(errors.to_string()));
    }

    let project_name = form.custom.custom_stack_code.clone();
    let metadata: Value = serde_json::to_value::<ProjectForm>(form)
        .or(serde_json::to_value::<ProjectForm>(ProjectForm::default()))
        .unwrap();

    let project = models::Project::new(user.id.clone(), project_name, metadata, request_json);

    db::project::insert(pg_pool.get_ref(), project)
        .await
        .map(|project| JsonResponse::build().set_item(project).ok("Ok"))
        .map_err(|_| JsonResponse::internal_server_error("Internal Server Error"))
}
