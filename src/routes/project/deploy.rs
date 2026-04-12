use crate::configuration::Settings;
use crate::connectors::{
    app_service_catalog,
    install_service::InstallServiceConnector, user_service::UserServiceConnector,
};
use crate::db;
use crate::forms;
use crate::helpers::project::builder::DcBuilder;
use crate::helpers::{JsonResponse, MqManager, VaultClient};
use crate::models;
use actix_web::{post, web, web::Data, Responder, Result};
use serde_valid::Validate;
use std::collections::HashSet;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

fn parse_template_requirements(
    template: &models::StackTemplate,
) -> Result<models::InfrastructureRequirements, String> {
    serde_json::from_value(template.infrastructure_requirements.clone()).map_err(|err| {
        tracing::error!(
            "Failed to parse infrastructure requirements for template {}: {}",
            template.id,
            err
        );
        "Template infrastructure requirements are invalid".to_string()
    })
}

fn validate_template_target_requirements(
    template: &models::StackTemplate,
    requirements: &models::InfrastructureRequirements,
    provider: &str,
    os: Option<&str>,
) -> Result<(), String> {
    let mut mismatches = Vec::new();

    if !requirements.supported_clouds.is_empty() {
        let supported: HashSet<String> = requirements
            .supported_clouds
            .iter()
            .map(|cloud| cloud.to_ascii_lowercase())
            .collect();
        if !supported.contains(&provider.to_ascii_lowercase()) {
            mismatches.push(format!(
                "cloud provider '{}' is not supported (allowed: {})",
                provider,
                requirements.supported_clouds.join(", ")
            ));
        }
    }

    if !requirements.supported_os.is_empty() {
        match os {
            Some(target_os)
                if requirements
                    .supported_os
                    .iter()
                    .any(|supported_os| supported_os.eq_ignore_ascii_case(target_os)) => {}
            Some(target_os) => mismatches.push(format!(
                "operating system '{}' is not supported (allowed: {})",
                target_os,
                requirements.supported_os.join(", ")
            )),
            None => mismatches.push(format!(
                "operating system is required (allowed: {})",
                requirements.supported_os.join(", ")
            )),
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Template '{}' cannot be deployed to this target: {}",
            template.slug,
            mismatches.join("; ")
        ))
    }
}

fn validate_min_ram_requirement(
    template: &models::StackTemplate,
    server_slug: &str,
    minimum_ram_mb: i32,
    server_capacity: &app_service_catalog::ServerCapacity,
) -> Result<(), String> {
    match server_capacity.ram_mb {
        Some(available_ram_mb) if available_ram_mb >= minimum_ram_mb => Ok(()),
        Some(available_ram_mb) => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' does not meet minimum RAM requirement (required: {} MB, available: {} MB)",
            template.slug, server_slug, minimum_ram_mb, available_ram_mb
        )),
        None => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' is missing RAM metadata",
            template.slug, server_slug
        )),
    }
}

fn validate_min_disk_requirement(
    template: &models::StackTemplate,
    server_slug: &str,
    minimum_disk_gb: i32,
    server_capacity: &app_service_catalog::ServerCapacity,
) -> Result<(), String> {
    match server_capacity.disk_gb {
        Some(available_disk_gb) if available_disk_gb >= minimum_disk_gb => Ok(()),
        Some(available_disk_gb) => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' does not meet minimum disk requirement (required: {} GB, available: {} GB)",
            template.slug, server_slug, minimum_disk_gb, available_disk_gb
        )),
        None => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' is missing disk metadata",
            template.slug, server_slug
        )),
    }
}

fn validate_min_cpu_requirement(
    template: &models::StackTemplate,
    server_slug: &str,
    minimum_cpu_cores: i32,
    server_capacity: &app_service_catalog::ServerCapacity,
) -> Result<(), String> {
    match server_capacity.cpu_cores {
        Some(available_cpu_cores) if available_cpu_cores >= minimum_cpu_cores => Ok(()),
        Some(available_cpu_cores) => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' does not meet minimum CPU requirement (required: {} cores, available: {} cores)",
            template.slug, server_slug, minimum_cpu_cores, available_cpu_cores
        )),
        None => Err(format!(
            "Template '{}' cannot be deployed to this target: selected server '{}' is missing CPU metadata",
            template.slug, server_slug
        )),
    }
}

async fn validate_template_server_capacity_requirements(
    template: &models::StackTemplate,
    requirements: &models::InfrastructureRequirements,
    provider: &str,
    cloud_id: Option<i32>,
    server_slug: Option<&str>,
    access_token: Option<&str>,
) -> Result<(), String> {
    if requirements.min_ram_mb.is_none()
        && requirements.min_disk_gb.is_none()
        && requirements.min_cpu_cores.is_none()
    {
        return Ok(());
    }

    if !app_service_catalog::is_supported_cloud_provider(provider) {
        return Ok(());
    }

    let server_slug = server_slug.ok_or_else(|| {
        format!(
            "Template '{}' cannot be deployed to this target: selected server is required for minimum RAM validation",
            template.slug
        )
    })?;

    let payload = app_service_catalog::fetch_catalog(provider, "servers", cloud_id, access_token)
        .await
        .map_err(|err| {
            format!(
                "Template '{}' cannot be deployed to this target: failed to load server catalog: {}",
                template.slug, err
            )
        })?;

    let server_capacity = app_service_catalog::resolve_server_capacity(&payload, server_slug)
        .ok_or_else(|| {
            format!(
                "Template '{}' cannot be deployed to this target: selected server '{}' was not found in the provider catalog",
                template.slug, server_slug
            )
        })?;

    if let Some(minimum_ram_mb) = requirements.min_ram_mb {
        validate_min_ram_requirement(template, server_slug, minimum_ram_mb, &server_capacity)?;
    }

    if let Some(minimum_disk_gb) = requirements.min_disk_gb {
        validate_min_disk_requirement(template, server_slug, minimum_disk_gb, &server_capacity)?;
    }

    if let Some(minimum_cpu_cores) = requirements.min_cpu_cores {
        validate_min_cpu_requirement(template, server_slug, minimum_cpu_cores, &server_capacity)?;
    }

    Ok(())
}

#[tracing::instrument(name = "Deploy for every user", skip_all)]
#[post("/{id}/deploy")]
pub async fn item(
    user: web::ReqData<Arc<models::User>>,
    path: web::Path<(i32,)>,
    mut form: web::Json<forms::project::Deploy>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    _sets: Data<Settings>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
    install_service: Data<Arc<dyn InstallServiceConnector>>,
    vault_client: Data<VaultClient>,
) -> Result<impl Responder> {
    let id = path.0;
    tracing::debug!("User {} is deploying project: {}", user.id, id);

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

    let marketplace_template = if let Some(template_id) = project.source_template_id {
        let template = db::marketplace::get_by_id(pg_pool.get_ref(), template_id)
            .await
            .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

        if let Some(template) = template {
            if let Some(required_plan) = template.required_plan_name.as_deref() {
                let has_plan = user_service
                    .user_has_plan(&user.id, required_plan, user.access_token.as_deref())
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

            Some(template)
        } else {
            None
        }
    } else {
        None
    };

    let id = project.id;

    form.cloud.user_id = Some(user.id.clone());
    form.cloud.project_id = Some(id);

    // Validate cloud credentials before encrypting/saving.
    // For cloud providers ("htz", "do", "lin", "aws", etc.) we need valid credentials.
    if form.cloud.provider != "own" {
        let token_empty = form
            .cloud
            .cloud_token
            .as_ref()
            .map_or(true, |t| t.is_empty());
        let key_empty = form.cloud.cloud_key.as_ref().map_or(true, |k| k.is_empty());
        let secret_empty = form
            .cloud
            .cloud_secret
            .as_ref()
            .map_or(true, |s| s.is_empty());

        if token_empty && (key_empty || secret_empty) {
            tracing::error!(
                "Deploy rejected: cloud provider '{}' requires credentials but none provided",
                form.cloud.provider
            );
            return Err(JsonResponse::<models::Project>::build().bad_request(
                "Cloud API credentials are required for cloud deployments. \
                 Please provide your cloud provider API token.",
            ));
        }
    }

    // Save cloud credentials if requested, capturing the returned cloud with its DB id
    let cloud_creds: models::Cloud = (&form.cloud).into();

    let cloud_creds = if Some(true) == cloud_creds.save_token {
        db::cloud::insert(pg_pool.get_ref(), cloud_creds.clone())
            .await
            .map_err(|_| {
                JsonResponse::<models::Cloud>::build()
                    .internal_server_error("Internal Server Error")
            })?
    } else {
        cloud_creds
    };

    // Handle server: if server_id provided, update existing; otherwise create new
    let server = if let Some(server_id) = form.server.server_id {
        // Update existing server
        let existing = db::server::fetch(pg_pool.get_ref(), server_id)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Failed to fetch server")
            })?
            .ok_or_else(|| JsonResponse::<models::Server>::build().not_found("Server not found"))?;

        // Verify ownership
        if existing.user_id != user.id {
            return Err(JsonResponse::<models::Server>::build().not_found("Server not found"));
        }

        let mut server = existing;
        server.disk_type = form.server.disk_type.clone();
        server.region = form.server.region.clone();
        server.server = form.server.server.clone();
        server.zone = form.server.zone.clone().or(server.zone);
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
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Failed to update server")
            })?
    } else {
        // Create new server
        let mut server: models::Server = (&form.server).into();
        server.user_id = user.id.clone();
        server.project_id = id;
        // Set cloud_id from saved cloud credentials (if cloud was saved, it has a DB id)
        if cloud_creds.id != 0 {
            server.cloud_id = Some(cloud_creds.id);
        }

        db::server::insert(pg_pool.get_ref(), server)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Internal Server Error")
            })?
    };

    if let Some(template) = marketplace_template.as_ref() {
        let requirements = parse_template_requirements(template)
            .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;

        validate_template_target_requirements(template, &requirements, &form.cloud.provider, server.os.as_deref())
        .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;

        validate_template_server_capacity_requirements(
            template,
            &requirements,
            &form.cloud.provider,
            if cloud_creds.id != 0 {
                Some(cloud_creds.id)
            } else {
                None
            },
            server.server.as_deref(),
            user.access_token.as_deref(),
        )
        .await
        .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;
    }

    // Build compose only after marketplace compatibility checks so unsupported
    // targets fail with a client error before we do deeper deployment work.
    let dc = DcBuilder::new(project);
    let fc = dc
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

    // Ensure every deploy payload carries the Stacker public key so the Install
    // Service can inject it into authorized_keys on the remote server.
    // - If the server has no active key yet: generate one, store in Vault, capture public key.
    // - If the key is already active: fetch the public key from Vault.
    let mut new_public_key: Option<String> = None;
    let mut captured_private_key: Option<String> = None;
    let server = if server.key_status != "active" {
        match VaultClient::generate_ssh_keypair() {
            Ok((public_key, private_key)) => {
                match vault_client
                    .get_ref()
                    .store_ssh_key(&user.id, server.id, &public_key, &private_key)
                    .await
                {
                    Ok(vault_path) => {
                        tracing::info!(
                            "Auto-generated SSH key for server {} (vault_key_path: {})",
                            server.id,
                            vault_path
                        );
                        new_public_key = Some(public_key);
                        captured_private_key = Some(private_key);
                        db::server::update_ssh_key_status(
                            pg_pool.get_ref(),
                            server.id,
                            Some(vault_path),
                            "active",
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to update SSH key status: {}", e);
                            server
                        })
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to store auto-generated SSH key in Vault for server {}: {}",
                            server.id,
                            e
                        );
                        server
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to auto-generate SSH keypair for server {}: {}",
                    server.id,
                    e
                );
                server
            }
        }
    } else {
        // Key already in Vault — fetch the public key so the Install Service can
        // append it to authorized_keys on every deploy (idempotent on the remote side).
        match vault_client
            .get_ref()
            .fetch_ssh_public_key(&user.id, server.id)
            .await
        {
            Ok(pk) => {
                tracing::info!(
                    "Fetched existing public key from Vault for server {}",
                    server.id
                );
                new_public_key = Some(pk);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch public key from Vault for server {}: {}",
                    server.id,
                    e
                );
            }
        }
        server
    };

    // For "own" flow (existing server with IP), SSH access is required.
    // If we couldn't set up the SSH key, fail early instead of letting the
    // Install Service crash when it cannot find the key.
    let has_existing_ip = server.srv_ip.as_ref().map_or(false, |ip| !ip.is_empty());
    if has_existing_ip && new_public_key.is_none() && server.vault_key_path.is_none() {
        tracing::error!(
            "Cannot deploy to existing server {} (IP: {:?}): SSH key is not available. \
             vault_key_path is None and key generation failed.",
            server.id,
            server.srv_ip,
        );
        return Err(JsonResponse::<models::Project>::build().bad_request(
            "SSH key is not available for this server. \
             Please generate an SSH key first with `stacker ssh-key generate` \
             or re-add your server with SSH credentials.",
        ));
    }

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.metadata.clone();
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let deployment = models::Deployment::new(
        dc.project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        String::from("pending"),
        "runc".to_string(),
        json_request,
    );

    let saved_deployment = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        })?;

    let deployment_id = saved_deployment.id;

    // For "own" flow, fetch the SSH private key from Vault so the Install Service
    // can SSH into the server directly without relying on Redis-cached file paths.
    let new_private_key = if server.vault_key_path.is_some() {
        // For newly generated keys, use the in-memory value to avoid an extra Vault round-trip.
        // For existing servers (re-deployment), fetch from Vault.
        if let Some(pk) = captured_private_key {
            Some(pk)
        } else {
            match vault_client
                .get_ref()
                .fetch_ssh_key(&user.id, server.id)
                .await
            {
                Ok(pk) => {
                    tracing::info!(
                        "Fetched SSH private key from Vault for server {}",
                        server.id
                    );
                    Some(pk)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch SSH private key from Vault for server {}: {}",
                        server.id,
                        e
                    );
                    None
                }
            }
        }
    } else {
        None
    };

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
            form.registry.clone(),
            fc,
            mq_manager.get_ref(),
            new_public_key,
            new_private_key,
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
#[tracing::instrument(name = "Deploy, when cloud token is saved", skip_all)]
#[post("/{id}/deploy/{cloud_id}")]
pub async fn saved_item(
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<forms::project::Deploy>,
    path: web::Path<(i32, i32)>,
    pg_pool: Data<PgPool>,
    mq_manager: Data<MqManager>,
    _sets: Data<Settings>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
    install_service: Data<Arc<dyn InstallServiceConnector>>,
    vault_client: Data<VaultClient>,
) -> Result<impl Responder> {
    let id = path.0;
    let cloud_id = path.1;

    tracing::debug!(
        "User {} is deploying project: {} to cloud: {}",
        user.id,
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

    let marketplace_template = if let Some(template_id) = project.source_template_id {
        let template = db::marketplace::get_by_id(pg_pool.get_ref(), template_id)
            .await
            .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

        if let Some(template) = template {
            if let Some(required_plan) = template.required_plan_name.as_deref() {
                let has_plan = user_service
                    .user_has_plan(&user.id, required_plan, user.access_token.as_deref())
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

            Some(template)
        } else {
            None
        }
    } else {
        None
    };

    let id = project.id;

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

    // Validate that saved cloud credentials can be decrypted before proceeding.
    // When SECURITY_KEY changed or encryption is corrupted, decode() silently
    // returns "" which causes a 401 deep inside the Install Service. Catch it early.
    if cloud.provider != "own" {
        let test_cloud = forms::cloud::CloudForm::decode_model(cloud.clone(), true);
        let token_empty = test_cloud
            .cloud_token
            .as_ref()
            .map_or(true, |t| t.is_empty());
        let key_empty = test_cloud.cloud_key.as_ref().map_or(true, |k| k.is_empty());
        let secret_empty = test_cloud
            .cloud_secret
            .as_ref()
            .map_or(true, |s| s.is_empty());

        // Most providers need cloud_token; AWS needs cloud_key + cloud_secret
        if token_empty && (key_empty || secret_empty) {
            tracing::error!(
                "Cloud credentials for cloud_id={} (provider={}) could not be decrypted. \
                 Token empty: {}, Key empty: {}, Secret empty: {}",
                cloud_id,
                cloud.provider,
                token_empty,
                key_empty,
                secret_empty,
            );
            return Err(JsonResponse::<models::Project>::build().bad_request(
                "Cloud API credentials could not be decrypted. \
                 Please delete and re-add your cloud credentials in Settings → Cloud Providers.",
            ));
        }
    }

    // Handle server: if server_id provided, update existing; otherwise create new
    let server = if let Some(server_id) = form.server.server_id {
        // Update existing server
        let existing = db::server::fetch(pg_pool.get_ref(), server_id)
            .await
            .map_err(|_| {
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Failed to fetch server")
            })?
            .ok_or_else(|| JsonResponse::<models::Server>::build().not_found("Server not found"))?;

        // Verify ownership
        if existing.user_id != user.id {
            return Err(JsonResponse::<models::Server>::build().not_found("Server not found"));
        }

        let mut server = existing;
        server.disk_type = form.server.disk_type.clone();
        server.region = form.server.region.clone();
        server.server = form.server.server.clone();
        server.zone = form.server.zone.clone().or(server.zone);
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
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Failed to update server")
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
                JsonResponse::<models::Server>::build()
                    .internal_server_error("Failed to create server")
            })?
    };

    if let Some(template) = marketplace_template.as_ref() {
        let requirements = parse_template_requirements(template)
            .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;

        validate_template_target_requirements(template, &requirements, &cloud.provider, server.os.as_deref())
            .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;

        validate_template_server_capacity_requirements(
            template,
            &requirements,
            &cloud.provider,
            Some(cloud_id),
            server.server.as_deref(),
            user.access_token.as_deref(),
        )
        .await
        .map_err(|msg| JsonResponse::<models::Project>::build().bad_request(msg))?;
    }

    // Build compose only after marketplace compatibility checks so unsupported
    // targets fail with a client error before we do deeper deployment work.
    let dc = DcBuilder::new(project);
    let fc = dc
        .build()
        .map_err(|err| JsonResponse::<models::Project>::build().internal_server_error(err))?;

    // Ensure every deploy payload carries the Stacker public key so the Install
    // Service can inject it into authorized_keys on the remote server.
    // - If the server has no active key yet: generate one, store in Vault, capture public key.
    // - If the key is already active: fetch the public key from Vault.
    let mut new_public_key: Option<String> = None;
    let mut captured_private_key: Option<String> = None;
    let server = if server.key_status != "active" {
        match VaultClient::generate_ssh_keypair() {
            Ok((public_key, private_key)) => {
                match vault_client
                    .get_ref()
                    .store_ssh_key(&user.id, server.id, &public_key, &private_key)
                    .await
                {
                    Ok(vault_path) => {
                        tracing::info!(
                            "Auto-generated SSH key for server {} (vault_key_path: {})",
                            server.id,
                            vault_path
                        );
                        new_public_key = Some(public_key);
                        captured_private_key = Some(private_key);
                        db::server::update_ssh_key_status(
                            pg_pool.get_ref(),
                            server.id,
                            Some(vault_path),
                            "active",
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to update SSH key status: {}", e);
                            server
                        })
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to store auto-generated SSH key in Vault for server {}: {}",
                            server.id,
                            e
                        );
                        server
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to auto-generate SSH keypair for server {}: {}",
                    server.id,
                    e
                );
                server
            }
        }
    } else {
        // Key already in Vault — fetch the public key so the Install Service can
        // append it to authorized_keys on every deploy (idempotent on the remote side).
        match vault_client
            .get_ref()
            .fetch_ssh_public_key(&user.id, server.id)
            .await
        {
            Ok(pk) => {
                tracing::info!(
                    "Fetched existing public key from Vault for server {}",
                    server.id
                );
                new_public_key = Some(pk);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch public key from Vault for server {}: {}",
                    server.id,
                    e
                );
            }
        }
        server
    };

    // For "own" flow (existing server with IP), SSH access is required.
    // If we couldn't set up the SSH key, fail early instead of letting the
    // Install Service crash when it cannot find the key.
    let has_existing_ip = server.srv_ip.as_ref().map_or(false, |ip| !ip.is_empty());
    if has_existing_ip && new_public_key.is_none() && server.vault_key_path.is_none() {
        tracing::error!(
            "Cannot deploy to existing server {} (IP: {:?}): SSH key is not available. \
             vault_key_path is None and key generation failed.",
            server.id,
            server.srv_ip,
        );
        return Err(JsonResponse::<models::Project>::build().bad_request(
            "SSH key is not available for this server. \
             Please generate an SSH key first with `stacker ssh-key generate` \
             or re-add your server with SSH credentials.",
        ));
    }

    // Store deployment attempts into deployment table in db
    let json_request = dc.project.metadata.clone();
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let deployment = models::Deployment::new(
        dc.project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        String::from("pending"),
        "runc".to_string(),
        json_request,
    );

    let result = db::deployment::insert(pg_pool.get_ref(), deployment)
        .await
        .map_err(|_| {
            JsonResponse::<models::Project>::build().internal_server_error("Internal Server Error")
        })?;

    let deployment_id = result.id;

    tracing::debug!("Save deployment result: {:?}", result);

    // For "own" flow, fetch the SSH private key from Vault so the Install Service
    // can SSH into the server directly without relying on Redis-cached file paths.
    let new_private_key = if server.vault_key_path.is_some() {
        // For newly generated keys, use the in-memory value to avoid an extra Vault round-trip.
        // For existing servers (re-deployment), fetch from Vault.
        if let Some(pk) = captured_private_key {
            Some(pk)
        } else {
            match vault_client
                .get_ref()
                .fetch_ssh_key(&user.id, server.id)
                .await
            {
                Ok(pk) => {
                    tracing::info!(
                        "Fetched SSH private key from Vault for server {}",
                        server.id
                    );
                    Some(pk)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch SSH private key from Vault for server {}: {}",
                        server.id,
                        e
                    );
                    None
                }
            }
        }
    } else {
        None
    };

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
            form.registry.clone(),
            fc,
            mq_manager.get_ref(),
            new_public_key,
            new_private_key,
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

#[cfg(test)]
mod tests {
    use super::{
        validate_min_cpu_requirement, validate_min_disk_requirement, validate_min_ram_requirement,
    };
    use crate::connectors::app_service_catalog::ServerCapacity;
    use crate::models;
    use serde_json::json;
    use uuid::Uuid;

    fn build_template(slug: &str) -> models::StackTemplate {
        models::StackTemplate {
            id: Uuid::new_v4(),
            creator_user_id: "creator".to_string(),
            creator_name: None,
            name: "Test template".to_string(),
            slug: slug.to_string(),
            short_description: None,
            long_description: None,
            category_code: None,
            product_id: None,
            tags: json!([]),
            tech_stack: json!({}),
            status: "approved".to_string(),
            is_configurable: None,
            view_count: None,
            deploy_count: None,
            required_plan_name: None,
            price: None,
            billing_cycle: None,
            currency: None,
            created_at: None,
            updated_at: None,
            approved_at: None,
            verifications: json!({}),
            infrastructure_requirements: json!({}),
        }
    }

    #[test]
    fn min_ram_validation_allows_exact_capacity_match() {
        let template = build_template("exact-match");
        let server_capacity = ServerCapacity {
            id: "t3.medium".to_string(),
            ram_mb: Some(2048),
            cpu_cores: Some(2),
            disk_gb: Some(40),
        };

        assert_eq!(
            Ok(()),
            validate_min_ram_requirement(&template, "t3.medium", 2048, &server_capacity)
        );
    }

    #[test]
    fn min_ram_validation_rejects_lower_capacity() {
        let template = build_template("needs-more-ram");
        let server_capacity = ServerCapacity {
            id: "t3.small".to_string(),
            ram_mb: Some(1024),
            cpu_cores: Some(2),
            disk_gb: Some(20),
        };

        let err = validate_min_ram_requirement(&template, "t3.small", 2048, &server_capacity)
            .expect_err("lower RAM should be rejected");

        assert!(err.contains("minimum RAM requirement"));
        assert!(err.contains("2048"));
        assert!(err.contains("1024"));
    }

    #[test]
    fn min_disk_validation_allows_exact_capacity_match() {
        let template = build_template("disk-exact-match");
        let server_capacity = ServerCapacity {
            id: "t3.medium".to_string(),
            ram_mb: Some(2048),
            cpu_cores: Some(2),
            disk_gb: Some(40),
        };

        assert_eq!(
            Ok(()),
            validate_min_disk_requirement(&template, "t3.medium", 40, &server_capacity)
        );
    }

    #[test]
    fn min_disk_validation_rejects_lower_capacity() {
        let template = build_template("needs-more-disk");
        let server_capacity = ServerCapacity {
            id: "t3.small".to_string(),
            ram_mb: Some(2048),
            cpu_cores: Some(2),
            disk_gb: Some(20),
        };

        let err = validate_min_disk_requirement(&template, "t3.small", 40, &server_capacity)
            .expect_err("lower disk should be rejected");

        assert!(err.contains("minimum disk requirement"));
        assert!(err.contains("40"));
        assert!(err.contains("20"));
    }

    #[test]
    fn min_cpu_validation_allows_exact_capacity_match() {
        let template = build_template("cpu-exact-match");
        let server_capacity = ServerCapacity {
            id: "t3.medium".to_string(),
            ram_mb: Some(4096),
            cpu_cores: Some(4),
            disk_gb: Some(40),
        };

        assert_eq!(
            Ok(()),
            validate_min_cpu_requirement(&template, "t3.medium", 4, &server_capacity)
        );
    }

    #[test]
    fn min_cpu_validation_rejects_lower_capacity() {
        let template = build_template("needs-more-cpu");
        let server_capacity = ServerCapacity {
            id: "t3.small".to_string(),
            ram_mb: Some(4096),
            cpu_cores: Some(2),
            disk_gb: Some(80),
        };

        let err = validate_min_cpu_requirement(&template, "t3.small", 4, &server_capacity)
            .expect_err("lower CPU should be rejected");

        assert!(err.contains("minimum CPU requirement"));
        assert!(err.contains("4"));
        assert!(err.contains("2"));
    }
}
