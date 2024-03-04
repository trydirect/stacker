use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{web, web::Data, Responder, Result, post};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

#[tracing::instrument(name = "Update project.")]
#[post("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    form: web::Json<forms::project::ProjectForm>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    let id = path.0;
    let mut project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) if project.user_id != user.id => {
                Err(JsonResponse::<models::Project>::build().bad_request("Project not found"))
            }
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("Project not found")),
        })?;

    tracing::debug!("form data: {:?}", form);
    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Project>::build().form_error(errors.to_string()));
    }

    let project_name = form.custom.custom_stack_code.clone();
    let form_inner = form.into_inner();

    if !form_inner.is_readable_docker_image().await.is_ok() {
        return Err(JsonResponse::<models::Project>::build().bad_request("Can not access docker image"));
    }

    let body: Value = serde_json::to_value::<forms::project::ProjectForm>(form_inner)
        .map_err(|err| 
            JsonResponse::<models::Project>::build().bad_request(format!("{err}"))
        )?;

    project.name = project_name;
    project.body = body;

    db::project::update(pg_pool.get_ref(), project)
        .await
        .map(|project| {
            JsonResponse::<models::Project>::build()
                .set_item(project)
                .ok("success")
        })
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            JsonResponse::<models::Project>::build().internal_server_error("")
        })
}
