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
#[post("/{id}/deploy/{cloud_id}")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32, i32)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    let cloud_id = path.1;
    //let cloud_id = Some(1);
    tracing::debug!("User {:?} is deploying project: {} to cloud: {} ", user, id, cloud_id);

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

    let cloud = match db::cloud::fetch(pg_pool.get_ref(), cloud_id).await {
        Ok(cloud) => {
            match cloud {
                Some(cloud) => cloud,
                None => {
                    return Err(JsonResponse::<models::Project>::build().not_found("No cloud configured"));
                }
            }
        }
        Err(e) => {
            return Err(JsonResponse::<models::Project>::build().not_found("No cloud configured"));
        }
    };

    let server = match db::server::fetch_by_project(pg_pool.get_ref(), dc.project.id.clone()).await {
        Ok(server) => {
            // for now we support only one type of servers
            // if let Some(server) = server.into_iter().nth(0) {
            //     server
            // }
            server.into_iter().nth(0).unwrap() // @todo refactoring is required
        }
        Err(err) => {
            return Err(JsonResponse::<models::Project>::build().not_found("No servers configured"));
        }
    };

    // let mut payload = forms::project::Payload::default();
    let mut payload = forms::project::Payload::try_from(&dc.project)
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))?;
    payload.server = Some(server.into());
    payload.cloud  = Some(cloud.into());
    payload.user_token = Some(user.id.clone());
    payload.user_email = Some(user.email.clone());
    // let compressed = fc.unwrap_or("".to_string());
    payload.docker_compose = Some(compress(fc.as_str()));


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
    tracing::debug!("Send project data <<<<<<<<<<<>>>>>>>>>>>>>>>>{:?}", payload);

    mq_manager
        .publish(
            "install".to_string(),
            "install.start.tfa.all.all".to_string(),
            &payload,
        )
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .map(|_| {
            JsonResponse::<models::Project>::build()
                .set_id(id)
                .ok("Success")
        })

}
