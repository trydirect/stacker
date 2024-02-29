use crate::configuration::Settings;
use crate::db;
use crate::forms;
use crate::helpers::project::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use crate::helpers::compressor::compress;



#[tracing::instrument(name = "Deploy for every user. Admin endpoint")]
#[post("/{id}/deploy")]
pub async fn add(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })?;

    let id = project.id.clone();
    let dc = DcBuilder::new(project);
    let fc = dc.build().map_err(|err| {
        JsonResponse::<models::Project>::build().internal_server_error(err)
    })?;

    let mut project_data = forms::project::Payload::try_from(&dc.project)
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))?;
    project_data.user_token = Some(user.id.clone());
    project_data.user_email = Some(user.email.clone());
    // let compressed = fc.unwrap_or("".to_string());
    project_data.docker_compose = Some(compress(fc.as_str()));

    // project_data.cloud =
    // project_data.server =

    let project_id = dc.project.id.clone();
    let json_request = dc.project.body.clone();
    let deployment = models::Deployment::new(
        project_id,
        String::from("pending"),
        json_request
    );

    let result = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map(|deployment| deployment)
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        });

    tracing::debug!("Save deployment result: {:?}", result);

    mq_manager
        .publish_and_confirm(
            "install".to_string(),
            "install.start.tfa.all.all".to_string(),
            &project_data,
        )
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .map(|_| {
            JsonResponse::<models::Project>::build()
                .set_id(id)
                .ok("Success")
        })

}
