use actix_web::{post, web, Responder, Result};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::sync::Arc;

use crate::cli::deployment_lock::DeploymentLock;
use crate::cli::stacker_client::DEFAULT_STACKER_URL;
use crate::db;
use crate::handoff::{
    DeploymentHandoffAgent, DeploymentHandoffCloud, DeploymentHandoffCredentials,
    DeploymentHandoffDeployment, DeploymentHandoffLink, DeploymentHandoffMintRequest,
    DeploymentHandoffMintResponse, DeploymentHandoffPayload, DeploymentHandoffProject,
    DeploymentHandoffResolveRequest, DeploymentHandoffServer,
};
use crate::helpers::JsonResponse;
use crate::models;
use crate::services::InMemoryHandoffStore;

const HANDOFF_TTL_MINUTES: i64 = 15;

#[post("/mint")]
pub async fn mint_handler(
    request: web::Json<DeploymentHandoffMintRequest>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    handoff_store: web::Data<Arc<InMemoryHandoffStore>>,
    http_request: actix_web::HttpRequest,
) -> Result<impl Responder> {
    let deployment = resolve_owned_deployment(pg_pool.get_ref(), &user.id, &request).await?;
    let project = db::project::fetch(pg_pool.get_ref(), deployment.project_id)
        .await
        .map_err(JsonResponse::<String>::internal_server_error)?
        .ok_or_else(|| JsonResponse::<String>::not_found("Project not found"))?;

    let server = db::server::fetch_by_project(pg_pool.get_ref(), project.id)
        .await
        .map_err(JsonResponse::<String>::internal_server_error)?
        .into_iter()
        .next();

    let expires_at = Utc::now() + Duration::minutes(HANDOFF_TTL_MINUTES);
    let payload = build_payload(&http_request, user.as_ref(), &project, &deployment, server.as_ref(), expires_at);
    let token = handoff_store.insert(payload);
    let base_url = resolve_public_base_url(&http_request);
    let link = DeploymentHandoffLink {
        token: token.clone(),
        url: format!("{}/handoff#{}", base_url.trim_end_matches('/'), token),
        expires_at,
    };

    Ok(JsonResponse::build()
        .set_item(DeploymentHandoffMintResponse {
            command: format!("stacker connect --handoff {}", token),
            token: link.token,
            url: link.url,
            expires_at: link.expires_at,
        })
        .ok("CLI handoff minted"))
}

#[post("/resolve")]
pub async fn resolve_handler(
    request: web::Json<DeploymentHandoffResolveRequest>,
    handoff_store: web::Data<Arc<InMemoryHandoffStore>>,
) -> Result<impl Responder> {
    let payload = handoff_store
        .resolve_once(&request.token)
        .ok_or_else(|| JsonResponse::<String>::not_found("Handoff token not found"))?;

    Ok(JsonResponse::build()
        .set_item(payload)
        .ok("CLI handoff resolved"))
}

async fn resolve_owned_deployment(
    pg_pool: &PgPool,
    user_id: &str,
    request: &DeploymentHandoffMintRequest,
) -> Result<models::Deployment> {
    let deployment = if let Some(deployment_id) = request.deployment_id {
        db::deployment::fetch(pg_pool, deployment_id)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?
    } else if let Some(deployment_hash) = request.deployment_hash.as_deref() {
        db::deployment::fetch_by_deployment_hash(pg_pool, deployment_hash)
            .await
            .map_err(JsonResponse::<String>::internal_server_error)?
    } else {
        return Err(JsonResponse::<String>::bad_request(
            "deployment_id or deployment_hash is required",
        ));
    };

    let deployment = deployment
        .ok_or_else(|| JsonResponse::<String>::not_found("Deployment not found"))?;
    if deployment.user_id.as_deref() != Some(user_id) {
        return Err(JsonResponse::<String>::not_found("Deployment not found"));
    }
    Ok(deployment)
}

fn build_payload(
    http_request: &actix_web::HttpRequest,
    user: &models::User,
    project: &models::Project,
    deployment: &models::Deployment,
    server: Option<&models::Server>,
    expires_at: chrono::DateTime<Utc>,
) -> DeploymentHandoffPayload {
    let target = infer_target(server);
    let ssh_user = server
        .and_then(|srv| srv.ssh_user.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| match target.as_str() {
            "cloud" | "server" => Some("root".to_string()),
            _ => None,
        });
    let ssh_port = server
        .and_then(|srv| srv.ssh_port)
        .map(|value| value as u16)
        .or_else(|| match target.as_str() {
            "cloud" | "server" => Some(22),
            _ => None,
        });

    let lockfile = serde_json::to_value(DeploymentLock {
        target: target.clone(),
        server_ip: server.and_then(|srv| srv.srv_ip.clone()),
        ssh_user: ssh_user.clone(),
        ssh_port,
        server_name: server.and_then(|srv| srv.name.clone().or_else(|| srv.server.clone())),
        deployment_id: Some(deployment.id as i64),
        project_id: Some(project.id as i64),
        cloud_id: server.and_then(|srv| srv.cloud_id),
        project_name: Some(project.name.clone()),
        deployed_at: deployment.updated_at.to_rfc3339(),
    })
    .unwrap_or_else(|_| serde_json::json!({}));

    let base_url = resolve_public_base_url(http_request);
    DeploymentHandoffPayload {
        version: 1,
        expires_at,
        project: DeploymentHandoffProject {
            id: project.id,
            name: project.name.clone(),
            identity: Some(project.name.clone()),
        },
        deployment: DeploymentHandoffDeployment {
            id: deployment.id,
            hash: deployment.deployment_hash.clone(),
            target,
            status: deployment.status.clone(),
        },
        server: server.map(|srv| DeploymentHandoffServer {
            ip: srv.srv_ip.clone(),
            ssh_user,
            ssh_port,
            name: srv.name.clone().or_else(|| srv.server.clone()),
        }),
        cloud: server.and_then(|srv| {
            srv.cloud_id.map(|cloud_id| DeploymentHandoffCloud {
                id: cloud_id,
                provider: None,
                region: srv.region.clone(),
            })
        }),
        lockfile,
        stacker_yml: Some(render_stacker_yml(project, deployment, server)),
        agent: server.and_then(|srv| {
            let server_ip = srv.srv_ip.clone()?;
            Some(DeploymentHandoffAgent {
                base_url: format!("http://{}:8080", server_ip),
                deployment_hash: deployment.deployment_hash.clone(),
            })
        }),
        credentials: user.access_token.clone().map(|access_token| DeploymentHandoffCredentials {
            access_token,
            token_type: "Bearer".to_string(),
            expires_at,
            email: Some(user.email.clone()),
            server_url: Some(base_url),
        }),
    }
}

fn infer_target(server: Option<&models::Server>) -> String {
    match server {
        Some(srv) if srv.cloud_id.is_some() => "cloud".to_string(),
        Some(_) => "server".to_string(),
        None => "local".to_string(),
    }
}

fn render_stacker_yml(
    project: &models::Project,
    deployment: &models::Deployment,
    server: Option<&models::Server>,
) -> String {
    let target = infer_target(server);
    let mut lines = vec![
        format!("name: {}", quote_yaml(&project.name)),
        "project:".to_string(),
        format!("  identity: {}", quote_yaml(&project.name)),
        "deploy:".to_string(),
        format!("  target: {}", quote_yaml(&target)),
        format!("  deployment_hash: {}", quote_yaml(&deployment.deployment_hash)),
    ];

    if let Some(srv) = server {
        if target == "server" {
            lines.push("  server:".to_string());
            if let Some(host) = srv.srv_ip.as_ref() {
                lines.push(format!("    host: {}", quote_yaml(host)));
            }
            if let Some(user) = srv.ssh_user.as_ref() {
                lines.push(format!("    user: {}", quote_yaml(user)));
            }
            if let Some(port) = srv.ssh_port {
                lines.push(format!("    port: {}", port));
            }
        } else if target == "cloud" {
            lines.push("  cloud:".to_string());
            if let Some(server_name) = srv.name.as_ref().or(srv.server.as_ref()) {
                lines.push(format!("    server: {}", quote_yaml(server_name)));
            }
        }
    }

    lines.join("\n") + "\n"
}

fn quote_yaml(value: &str) -> String {
    serde_yaml::to_string(value)
        .map(|yaml| yaml.trim().to_string())
        .unwrap_or_else(|_| format!("{:?}", value))
}

fn resolve_public_base_url(request: &actix_web::HttpRequest) -> String {
    let connection_info = request.connection_info();
    let scheme = connection_info.scheme();
    let host = connection_info.host();
    if !host.is_empty() {
        format!("{}://{}", scheme, host)
    } else {
        DEFAULT_STACKER_URL.to_string()
    }
}
