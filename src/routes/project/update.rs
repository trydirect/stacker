use std::str::FromStr;
use crate::forms::project::ProjectForm;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{web, Responder, Result, put};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use std::str;

#[tracing::instrument(name = "Update project.")]
#[put("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    web::Json(request_json): web::Json<serde_json::Value>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let mut project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)
        .and_then(|project| match project {
            Some(project) if project.user_id != user.id => {
                Err(JsonResponse::bad_request("Project not found"))
            }
            Some(project) => Ok(project),
            None => Err(JsonResponse::not_found("Project not found")),
        })?;

    // @todo ACL
    let form: ProjectForm= serde_json::from_value(request_json.clone())
        .map_err(|err| JsonResponse::bad_request(err.to_string()))?;
    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err();
        return Err(JsonResponse::bad_request(errors.to_string()));
    }

    let project_name = form.custom.custom_stack_code.clone();
    if !form.is_readable_docker_image().await.is_ok() {
        return Err(JsonResponse::bad_request("Can not access docker image"));
    }

    let body: Value = serde_json::to_value::<ProjectForm>(form)
        .or(serde_json::to_value::<ProjectForm>(ProjectForm::default()))
        .unwrap();


    project.name = project_name;
    project.body = body;
    project.request_json = request_json;

    db::project::update(pg_pool.get_ref(), project)
        .await
        .map(|project| {
            JsonResponse::<models::Project>::build()
                .set_item(project)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::internal_server_error("")
        })
}
