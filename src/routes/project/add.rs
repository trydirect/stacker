use crate::db;
use crate::forms;
use crate::helpers::JsonResponse;
use crate::models;
use actix_web::Error;
use actix_web::{
    post, web,
    web::{Bytes, Data},
    Responder, Result,
};
use serde_json::Value;
use serde_valid::Validate;
use sqlx::PgPool;
use std::str;
use std::sync::Arc;
use std::str::FromStr;

#[tracing::instrument(name = "Add project.")]
#[post("")]
pub async fn item(
    body: Bytes,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: Data<PgPool>,
) -> Result<impl Responder> {
    // @todo ACL
    let form = body_into_form(body.clone()).await?;
    let project_name = form.custom.custom_stack_code.clone();

    //let request_json = Some(serde_json::Value::from_str(r#"{"somefield": "somevalue"}"#).unwrap());
    // let request_json = form.request_json.clone();
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


async fn body_into_form(body: Bytes) -> Result<forms::project::ProjectForm, Error> {
    let body_bytes = actix_web::body::to_bytes(body).await.unwrap();
    let body_str = str::from_utf8(&body_bytes)
        .map_err(|err| JsonResponse::<forms::project::ProjectForm>::build().internal_server_error(err.to_string()))?;
    let deserializer = &mut serde_json::Deserializer::from_str(body_str);
    serde_path_to_error::deserialize(deserializer)
        .map_err(|err| {
            let msg = format!("{}:{:?}", err.path().to_string(), err);
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
        })
}

