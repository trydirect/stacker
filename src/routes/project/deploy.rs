use crate::configuration::Settings;
use crate::db;
use crate::forms;
use crate::helpers::project::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;
use serde_valid::Validate;
use crate::helpers::compressor::compress;
use chrono::{Utc};



#[tracing::instrument(name = "Deploy for every user")]
#[post("/{id}/deploy")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    mut form: web::Json<forms::project::Deploy>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    tracing::debug!("User {:?} is deploying project: {}", user, id);

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err().to_string();
        let err_msg = format!("Invalid form data received {:?}", &errors);
        tracing::debug!(err_msg);

        return Err(JsonResponse::<models::Project>::build().form_error(errors));
    }

    // Validate project
    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("not found")),
        })?;

    // Build compose
    let id = project.id;
    let dc = DcBuilder::new(project);
    let fc = dc.build().map_err(|err| {
        JsonResponse::<models::Project>::build().internal_server_error(err)
    })?;

    form.cloud.user_id = Some(user.id.clone());
    form.cloud.project_id = Some(id);
    // Save cloud credentials if requested
    let cloud_creds: models::Cloud = (&form.cloud).into();

    // let cloud_creds = forms::Cloud::decode_model(cloud_creds, false);

    if Some(true) == cloud_creds.save_token {
        db::cloud::insert(pg_pool.get_ref(), cloud_creds.clone())
            .await
            .map(|cloud| cloud)
            .map_err(|_| {
                JsonResponse::<models::Cloud>::build().internal_server_error("Internal Server Error")
            })?;
    }

    // Save server type and region
    let mut server: models::Server = (&form.server).into();
    server.user_id = user.id.clone();
    server.project_id = id;
    let server = db::server::insert(pg_pool.get_ref(), server)
        .await
        .map(|server| server)
        .map_err(|_| {
            JsonResponse::<models::Server>::build().internal_server_error("Internal Server Error")
        })?;

    // Build Payload for the 3-d party service through RabbitMQ
    let mut payload = forms::project::Payload::try_from(&dc.project)
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))?;

    payload.server = Some(server.into());
    payload.cloud  = Some(cloud_creds.into());
    payload.stack  = form.stack.clone().into();
    payload.user_token = Some(user.id.clone());
    payload.user_email = Some(user.email.clone());
    payload.docker_compose = Some(compress(fc.as_str()));

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.body.clone();
    let deployment = models::Deployment::new(
        dc.project.id,
        String::from("pending"),
        json_request
    );

    let result = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map(|deployment| {
            payload.id = Some(deployment.id);
            deployment
        }
        )
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        });

    tracing::debug!("Save deployment result: {:?}", result);
    tracing::debug!("Send project data <<<>>>{:?}", payload);

    // Send Payload
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
#[tracing::instrument(name = "Deploy, when cloud token is saved")]
#[post("/{id}/deploy/{cloud_id}")]
pub async fn saved_item(
    user: web::ReqData<Arc<models::User>>,
    mut form: web::Json<forms::project::Deploy>,
    path: web::Path<(i32, i32)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
) -> Result<impl Responder> {
    let id = path.0;
    let cloud_id = path.1;

    tracing::debug!("User {:?} is deploying project: {} to cloud: {} ", user, id, cloud_id);

    if !form.validate().is_ok() {
        let errors = form.validate().unwrap_err().to_string();
        let err_msg = format!("Invalid form data received {:?}", &errors);
        tracing::debug!(err_msg);

        return Err(JsonResponse::<models::Project>::build().form_error(errors));
    }

    // Validate project
    let project = db::project::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
        .and_then(|project| match project {
            Some(project) => Ok(project),
            None => Err(JsonResponse::<models::Project>::build().not_found("Project not found")),
        })?;

    // Build compose
    let id = project.id;
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
        Err(_e) => {
            return Err(JsonResponse::<models::Project>::build().not_found("No cloud configured"));
        }
    };

    let server = match db::server::fetch_by_project(pg_pool.get_ref(), dc.project.id.clone()).await {
        Ok(server) => {
            // currently we support only one type of servers
            //@todo multiple server types support
            match server.into_iter().nth(0) {
                Some(mut server) =>  {
                    // new updates
                    server.disk_type = form.server.disk_type.clone();
                    server.region = form.server.region.clone();
                    server.server = form.server.server.clone();
                    server.zone = form.server.zone.clone();
                    server.os = form.server.os.clone();
                    server.user_id = user.id.clone();
                    server.project_id = id;
                    server
                },
                None => {
                    // Create new server
                    let mut server: models::Server = (&form.server).into();
                    server.user_id = user.id.clone();
                    server.project_id = id;
                    db::server::insert(pg_pool.get_ref(), server)
                        .await
                        .map(|server| server)
                        .map_err(|_| {
                            JsonResponse::<models::Server>::build().internal_server_error("Internal Server Error")
                        })?
                }
            }
        }
        Err(_e) => {
            return Err(JsonResponse::<models::Project>::build().not_found("No servers configured"));
        }
    };

    let server = db::server::update(pg_pool.get_ref(), server)
        .await
        .map(|server| server)
        .map_err(|_| {
            JsonResponse::<models::Server>::build().internal_server_error("Internal Server Error")
        })?;

    // Building Payload for the 3-d party service through RabbitMQ
    // let mut payload = forms::project::Payload::default();
    let mut payload = forms::project::Payload::try_from(&dc.project)
        .map_err(|err| JsonResponse::<models::Project>::build().bad_request(err))?;

    payload.server = Some(server.into());
    payload.cloud  = Some(cloud.into());
    payload.stack  = form.stack.clone().into();
    payload.user_token = Some(user.id.clone());
    payload.user_email = Some(user.email.clone());
    payload.docker_compose = Some(compress(fc.as_str()));

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.body.clone();
    let deployment = models::Deployment::new(
        dc.project.id,
        String::from("pending"),
        json_request
    );

    let result = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map(|deployment| {
            payload.id = Some(deployment.id);
            deployment
        })
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        });

    tracing::debug!("Save deployment result: {:?}", result);
    tracing::debug!("Send project data <<<>>>{:?}", payload);

    // Send Payload
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



