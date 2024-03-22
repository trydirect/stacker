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
use serde_valid::Validate;

#[tracing::instrument(name = "Add project.")]
#[post("")]
pub async fn item(
    request_json: web::Json<serde_json::Value>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let request_json = request_json.into_inner(); //todo
    let form = serde_json::from_value(request_json.clone())
        .map_err(|err| {
            let msg = format!("{}", err); //todo JsonReponse::BadRequest::from(err)
            JsonResponse::<forms::project::ProjectForm>::build().bad_request(msg)
        })
        .and_then(|mut form: forms::project::ProjectForm| {
            if !form.validate().is_ok() {
                let errors = form.validate().unwrap_err().to_string();
                let err_msg = format!("Invalid data received {:?}", &errors);
                tracing::debug!(err_msg);

                return Err(JsonResponse::<models::Project>::build().form_error(errors));
            }

            Ok(form)
        })?;

    let project_name = form.custom.custom_stack_code.clone();

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
