use std::str::FromStr;
use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use crate::db;
use actix_web::{web, web::Data, Responder, Result, put};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use actix_web::web::Bytes;
use tracing::Instrument;
use std::str;

#[tracing::instrument(name = "Update project.")]
#[put("/{id}")]
pub async fn item(
    path: web::Path<(i32,)>,
    body: Bytes,
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

    let body_bytes = actix_web::body::to_bytes(body.clone()).await.unwrap();
    let body_str = str::from_utf8(&body_bytes)
        .map_err(|err| JsonResponse::<forms::project::ProjectForm>::build().internal_server_error(err.to_string()))?;
    let request_json = Value::from_str(body_str)?;
    tracing::debug!("Request json: {:?}", request_json);

    // @todo ACL
    let form = forms::project::form::body_into_form(body.clone()).await?;
    tracing::debug!("form data: {:?}", form);

    if let Err(errors) = form.validate() {
        return Err(JsonResponse::<models::Project>::build().form_error(errors.to_string()));
    }

    let project_name = form.custom.custom_stack_code.clone();

    if !form.is_readable_docker_image().await.is_ok() {
        return Err(JsonResponse::<models::Project>::build().bad_request("Can not access docker image"));
    }

    let body: Value = serde_json::to_value::<forms::project::ProjectForm>(form)
        .or(serde_json::to_value::<forms::project::ProjectForm>(forms::project::ProjectForm::default()))
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
            JsonResponse::<models::Project>::build().internal_server_error("")
        })
}
