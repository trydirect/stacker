use crate::configuration::Settings;
use crate::connectors::{
    install_service::InstallServiceConnector, user_service::UserServiceConnector,
};
use crate::db;
use crate::forms;
use crate::helpers::project::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use serde_valid::Validate;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[tracing::instrument(name = "Deploy for every user", skip(user_service, install_service))]
#[post("/{id}/deploy")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    mut form: web::Json<forms::project::Deploy>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
    install_service: Data<Arc<dyn InstallServiceConnector>>,
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

    // Check marketplace template plan requirements if project was created from template
    if let Some(template_id) = project.source_template_id {
        if let Some(template) = db::marketplace::get_by_id(pg_pool.get_ref(), template_id)
            .await
            .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?
        {
            // If template requires a specific plan, validate user has it
            if let Some(required_plan) = template.required_plan_name {
                let has_plan = user_service
                    .user_has_plan(&user.id, &required_plan)
                    .await
                    .map_err(|err| {
                        tracing::error!("Failed to validate plan: {:?}", err);
                        JsonResponse::<models::Project>::build()
                            .internal_server_error("Failed to validate subscription plan")
                    })?;

                if !has_plan {
                    tracing::warn!(
                        "User {} lacks required plan {} to deploy template {}",
                        user.id,
                        required_plan,
                        template_id
                    );
                    return Err(JsonResponse::<models::Project>::build().forbidden(format!(
                        "You require a '{}' subscription to deploy this template",
                        required_plan
                    )));
                }
            }
        }
    }

    // Build compose
    let id = project.id;
    let dc = DcBuilder::new(project);
    let fc = dc
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

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
                JsonResponse::<models::Cloud>::build()
                    .internal_server_error("Internal Server Error")
            })?;
    }

    // Handle server: if server_id provided, update existing; otherwise create new
    let server = if let Some(server_id) = form.server.server_id {
        // Update existing server
        let existing = db::server::fetch(pg_pool.get_ref(), server_id)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Failed to fetch server")
            })?
            .ok_or_else(|| {
                JsonResponse::<models::Server>::build().not_found("Server not found")
            })?;

        // Verify ownership
        if existing.user_id != user.id {
            return Err(JsonResponse::<models::Server>::build().not_found("Server not found"));
        }

        let mut server = existing;
        server.disk_type = form.server.disk_type.clone();
        server.region = form.server.region.clone();
        server.server = form.server.server.clone();
        server.zone = form.server.zone.clone();
        server.os = form.server.os.clone();
        server.project_id = id;
        // Preserve existing srv_ip if form doesn't provide one
        server.srv_ip = form.server.srv_ip.clone().or(server.srv_ip);
        server.ssh_user = form.server.ssh_user.clone().or(server.ssh_user);
        server.ssh_port = form.server.ssh_port.or(server.ssh_port);
        server.name = form.server.name.clone().or(server.name);
        if form.server.connection_mode.is_some() {
            server.connection_mode = form.server.connection_mode.clone().unwrap();
        }

        db::server::update(pg_pool.get_ref(), server)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Failed to update server")
            })?
    } else {
        // Create new server
        let mut server: models::Server = (&form.server).into();
        server.user_id = user.id.clone();
        server.project_id = id;

        db::server::insert(pg_pool.get_ref(), server)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Internal Server Error")
            })?
    };

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.metadata.clone();
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let deployment = models::Deployment::new(
        dc.project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        String::from("pending"),
        json_request,
    );

    let saved_deployment = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        })?;

    let deployment_id = saved_deployment.id;

    // Delegate to install service connector
    install_service
        .deploy(
            user.id.clone(),
            user.email.clone(),
            id,
            deployment_id,
            deployment_hash,
            &dc.project,
            cloud_creds,
            server,
            &form.stack,
            fc,
            mq_manager.get_ref(),
        )
        .await
        .map(|project_id| {
            JsonResponse::<models::Project>::build()
                .set_id(project_id)
                .set_meta(serde_json::json!({ "deployment_id": deployment_id }))
                .ok("Success")
        })
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
}
#[tracing::instrument(name = "Deploy, when cloud token is saved", skip(user_service, install_service))]
#[post("/{id}/deploy/{cloud_id}")]
pub async fn saved_item(
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::project::Deploy>,
    path: web::Path<(i32, i32)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    sets: Data<Settings>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
    install_service: Data<Arc<dyn InstallServiceConnector>>,
) -> Result<impl Responder> {
    let id = path.0;
    let cloud_id = path.1;

    tracing::debug!(
        "User {:?} is deploying project: {} to cloud: {} ",
        user,
        id,
        cloud_id
    );

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

    // Check marketplace template plan requirements if project was created from template
    if let Some(template_id) = project.source_template_id {
        if let Some(template) = db::marketplace::get_by_id(pg_pool.get_ref(), template_id)
            .await
            .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?
        {
            // If template requires a specific plan, validate user has it
            if let Some(required_plan) = template.required_plan_name {
                let has_plan = user_service
                    .user_has_plan(&user.id, &required_plan)
                    .await
                    .map_err(|err| {
                        tracing::error!("Failed to validate plan: {:?}", err);
                        JsonResponse::<models::Project>::build()
                            .internal_server_error("Failed to validate subscription plan")
                    })?;

                if !has_plan {
                    tracing::warn!(
                        "User {} lacks required plan {} to deploy template {}",
                        user.id,
                        required_plan,
                        template_id
                    );
                    return Err(JsonResponse::<models::Project>::build().forbidden(format!(
                        "You require a '{}' subscription to deploy this template",
                        required_plan
                    )));
                }
            }
        }
    }

    // Build compose
    let id = project.id;
    let dc = DcBuilder::new(project);
    let fc = dc
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

    let cloud = match db::cloud::fetch(pg_pool.get_ref(), cloud_id).await {
        Ok(cloud) => match cloud {
            Some(cloud) => cloud,
            None => {
                return Err(
                    JsonResponse::<models::Project>::build().not_found("No cloud configured")
                );
            }
        },
        Err(_e) => {
            return Err(JsonResponse::<models::Project>::build().not_found("No cloud configured"));
        }
    };

    // Handle server: if server_id provided, update existing; otherwise create new
    let server = if let Some(server_id) = form.server.server_id {
        // Update existing server
        let existing = db::server::fetch(pg_pool.get_ref(), server_id)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Failed to fetch server")
            })?
            .ok_or_else(|| {
                JsonResponse::<models::Server>::build().not_found("Server not found")
            })?;

        // Verify ownership
        if existing.user_id != user.id {
            return Err(JsonResponse::<models::Server>::build().not_found("Server not found"));
        }

        let mut server = existing;
        server.disk_type = form.server.disk_type.clone();
        server.region = form.server.region.clone();
        server.server = form.server.server.clone();
        server.zone = form.server.zone.clone();
        server.os = form.server.os.clone();
        server.project_id = id;
        // Preserve existing srv_ip if form doesn't provide one
        server.srv_ip = form.server.srv_ip.clone().or(server.srv_ip);
        server.ssh_user = form.server.ssh_user.clone().or(server.ssh_user);
        server.ssh_port = form.server.ssh_port.or(server.ssh_port);
        server.name = form.server.name.clone().or(server.name);
        if form.server.connection_mode.is_some() {
            server.connection_mode = form.server.connection_mode.clone().unwrap();
        }
        server.cloud_id = Some(cloud_id);

        db::server::update(pg_pool.get_ref(), server)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Failed to update server")
            })?
    } else {
        // Create new server
        let mut server: models::Server = (&form.server).into();
        server.user_id = user.id.clone();
        server.project_id = id;
        server.cloud_id = Some(cloud_id);

        db::server::insert(pg_pool.get_ref(), server)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build().internal_server_error("Failed to create server")
            })?
    };

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.metadata.clone();
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let deployment = models::Deployment::new(
        dc.project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        String::from("pending"),
        json_request,
    );

    let result = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        })?;

    let deployment_id = result.id;

    tracing::debug!("Save deployment result: {:?}", result);

    // Delegate to install service connector (determines own vs tfa routing)
    install_service
        .deploy(
            user.id.clone(),
            user.email.clone(),
            id,
            deployment_id,
            deployment_hash,
            &dc.project,
            cloud,
            server,
            &form.stack,
            fc,
            mq_manager.get_ref(),
        )
        .await
        .map(|project_id| {
            JsonResponse::<models::Project>::build()
                .set_id(project_id)
                .set_meta(serde_json::json!({ "deployment_id": deployment_id }))
                .ok("Success")
        })
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))
}
