//! `POST /api/admin/bake` and `GET /api/admin/bake/{id}`: bake a deployed
//! build box into a Quick Deploy snapshot without the operator typing its IP,
//! SSH key or version.
//!
//! Everything that can be checked without touching the box is checked before
//! the job starts, and answered at once: the deployment (the caller's own,
//! not a Quick Deploy server) and its server, the approved template with a
//! usable contract, a health URL on the box, and the box being one Hetzner
//! knows. Only then does the job connect, check that
//! every service is up, sanitize the box, snapshot it and record the snapshot.
//! The box is never removed: the result lists it for the operator.

use std::collections::BTreeSet;
use std::sync::Arc;

use actix_web::{get, post, web, HttpResponse, Responder, Result};
use serde::Deserialize;
use sqlx::PgPool;

use crate::connectors::config::HetznerConfig;
use crate::connectors::hetzner::{
    HetznerCloudClient, HetznerCloudConnector, HetznerSnapshotTarget,
};
use crate::db;
use crate::helpers::bake_job::{
    check_compose_services, check_health_url, probe_health, project_dir_from_compose_ls,
};
use crate::helpers::{JsonResponse, VaultClient};
use crate::models;

#[derive(Debug, Deserialize)]
pub struct BakeRequest {
    /// The build box's deployment (from the CLI deployment lock).
    pub deployment_id: i32,
    /// Approved marketplace template slug; its latest version is baked.
    pub stack: String,
    /// The stack's health endpoint on the build box.
    pub health_url: String,
    /// The build box's Hetzner server id, as the `bake` binary takes it.
    /// Needed when the project holds several server records - every
    /// `deploy --force-new` adds one, and Hetzner reuses IPs.
    #[serde(default)]
    pub server_id: Option<i64>,
}

/// What the job needs, all resolved and checked before it starts.
struct Prepared {
    server: models::Server,
    server_ip: String,
    version: String,
    contract: serde_json::Value,
    parsed_contract: crate::cli::config_parser::ConfigContract,
    protected_keys: BTreeSet<String>,
    token: String,
    connector: Arc<dyn HetznerCloudConnector>,
    target: HetznerSnapshotTarget,
}

async fn prepare(
    pool: &PgPool,
    admin: &models::User,
    form: &BakeRequest,
) -> Result<Prepared, actix_web::Error> {
    let deployment = db::deployment::fetch(pool, form.deployment_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("deployment not found"))?;

    // Sanitizing wipes the box, so a bake takes only the caller's own build box:
    // a mistyped id must not reach someone else's server. A service token owns
    // no box. A Quick Deploy server is a buyer's even when the caller made it.
    if deployment.user_id.as_deref() != Some(admin.id.as_str()) {
        return Err(JsonResponse::forbidden(
            "a bake takes only your own build box; this deployment is not yours",
        ));
    }
    if db::managed_usage::fetch_by_hash(pool, &deployment.deployment_hash)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .is_some()
    {
        return Err(JsonResponse::<()>::build()
            .conflict("this deployment is a Quick Deploy server, not a build box"));
    }

    let servers: Vec<models::Server> = db::server::fetch_by_project(pool, deployment.project_id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .into_iter()
        .filter(|s| {
            s.srv_ip
                .as_deref()
                .map_or(false, |ip| !ip.trim().is_empty())
        })
        .collect();
    if servers.is_empty() {
        return Err(JsonResponse::<()>::build()
            .conflict("the deployment's project has no server with an IP"));
    }
    if servers.len() > 1 && form.server_id.is_none() {
        return Err(JsonResponse::<()>::build().conflict(
            "the deployment's project has more than one server; pass server_id, the build \
             box's Hetzner server id",
        ));
    }

    // The latest approved version: the contract describes it, so version and
    // contract cannot disagree.
    let (template, version) = db::marketplace::get_by_slug_with_latest(pool, &form.stack)
        .await
        .map_err(|_| JsonResponse::not_found("no approved template with this slug"))?;
    let version = version
        .map(|v| v.version)
        .ok_or_else(|| JsonResponse::<()>::build().conflict("the template has no version"))?;
    let contract = db::marketplace::get_config_contract(pool, template.id)
        .await
        .map_err(JsonResponse::internal_server_error)?;
    if contract.is_null() {
        return Err(JsonResponse::<()>::build()
            .conflict("the template declares no config_contract; nothing would be sanitized"));
    }
    let protected_keys = crate::helpers::bake_finalize::protected_keys_from_contract(&contract);
    let parsed_contract = serde_json::from_value(contract.clone()).map_err(|e| {
        JsonResponse::<()>::build().conflict(format!(
            "the stored config_contract could not be parsed: {e}"
        ))
    })?;
    crate::helpers::bake_finalize::check_contract_usable(&protected_keys, false)
        .map_err(|e| JsonResponse::<()>::build().conflict(e.to_string()))?;

    let htz = HetznerConfig::from_env();
    let token = htz.token.filter(|t| !t.trim().is_empty()).ok_or_else(|| {
        JsonResponse::internal_server_error("HETZNER_TOKEN is not set on the stacker backend")
    })?;
    let client = HetznerCloudClient::new(htz.base_url)
        .map_err(|e| JsonResponse::internal_server_error(e.to_string()))?;
    let connector: Arc<dyn HetznerCloudConnector> = Arc::new(client);

    // Sanitizing removes SSH access, so the box must be the one Hetzner has at
    // the record's IP - and, when server_id is given, that very server -
    // before anything on it changes. Newest record first: an older record at
    // the same IP belongs to a removed box.
    let mut newest_first = servers.clone();
    newest_first.sort_by_key(|s| std::cmp::Reverse(s.id));
    let mut tried = BTreeSet::new();
    let mut chosen = None;
    for candidate in &newest_first {
        let ip = candidate.srv_ip.clone().unwrap_or_default();
        if !tried.insert(ip.clone()) {
            continue;
        }
        let target = HetznerSnapshotTarget {
            provider_server_id: None,
            server_name: None,
            public_ip: Some(ip.clone()),
        };
        match connector.resolve_snapshot_target(&token, &target).await {
            Ok(id) if form.server_id.map_or(true, |wanted| wanted == id) => {
                chosen = Some((candidate.clone(), id, target));
                break;
            }
            Ok(_) => continue,
            Err(e) if servers.len() == 1 => {
                return Err(JsonResponse::<()>::build().conflict(format!(
                    "Hetzner does not know a server at {ip} in the platform's project: {e}"
                )))
            }
            Err(_) => continue,
        }
    }
    let (server, provider_server_id, target) = chosen.ok_or_else(|| {
        JsonResponse::<()>::build().conflict(format!(
            "Hetzner server {} is not at the IP of any server of this deployment's project",
            form.server_id.unwrap_or_default()
        ))
    })?;
    let server_ip = server.srv_ip.clone().unwrap_or_default();
    check_health_url(&form.health_url, &server_ip).map_err(JsonResponse::bad_request)?;
    let target = HetznerSnapshotTarget {
        provider_server_id: Some(provider_server_id),
        ..target
    };

    Ok(Prepared {
        server,
        server_ip,
        version,
        contract,
        parsed_contract,
        protected_keys,
        token,
        connector,
        target,
    })
}

#[tracing::instrument(name = "Start a bake (admin)", skip_all)]
#[post("")]
pub async fn start_handler(
    admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    form: web::Json<BakeRequest>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let form = form.into_inner();
    let prepared = prepare(pg_pool.get_ref(), &admin, &form).await?;

    if db::bake_job::running_for_server(pg_pool.get_ref(), prepared.server.id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .is_some()
    {
        return Err(
            JsonResponse::<()>::build().conflict("a bake of this build box is already running")
        );
    }

    let job = db::bake_job::insert(
        pg_pool.get_ref(),
        db::bake_job::NewBakeJob {
            stack: &form.stack,
            version: &prepared.version,
            deployment_id: form.deployment_id,
            server_id: prepared.server.id,
            server_ip: &prepared.server_ip,
            created_by: &admin.id,
        },
    )
    .await
    .map_err(JsonResponse::internal_server_error)?;

    let pool = pg_pool.get_ref().clone();
    let vault = vault_client.clone(); // web::Data: an Arc, moved into the job
    let job_id = job.id;
    let stack = form.stack.clone();
    let health_url = form.health_url.clone();
    tokio::spawn(async move {
        let outcome = run_job(&pool, &vault, &stack, &health_url, prepared).await;
        let saved = match outcome {
            Ok((image_id, snapshot_id)) => {
                tracing::info!(%job_id, image_id, "bake finished");
                db::bake_job::mark_done(&pool, job_id, image_id, snapshot_id).await
            }
            Err(err) => {
                tracing::warn!(%job_id, error = %err, "bake failed");
                db::bake_job::mark_failed(&pool, job_id, &err).await
            }
        };
        if let Err(err) = saved {
            tracing::error!(%job_id, error = %err, "could not save the bake result");
        }
    });

    Ok(HttpResponse::Accepted().json(serde_json::json!({ "message": "accepted", "item": job })))
}

/// Connect, check, sanitize, snapshot, record. Returns (image_id, snapshot row id).
async fn run_job(
    pool: &PgPool,
    vault: &VaultClient,
    stack: &str,
    health_url: &str,
    p: Prepared,
) -> Result<(i64, i32), String> {
    use crate::helpers::ssh_client::{disconnect_ssh, exec_remote, open_ssh, HostKeyPolicy};

    probe_health(health_url).await?;

    let private_key_pem = vault
        .fetch_ssh_key(&p.server.user_id, p.server.id)
        .await
        .map_err(|e| format!("SSH key of the build box: {e}"))?;
    let user = p
        .server
        .ssh_user
        .clone()
        .unwrap_or_else(|| "root".to_string());
    let port = p
        .server
        .ssh_port
        .and_then(|v| u16::try_from(v).ok())
        .unwrap_or(22);

    let session = open_ssh(
        &p.server_ip,
        port,
        &user,
        &private_key_pem,
        std::time::Duration::from_secs(30),
        &HostKeyPolicy::Pin(p.server.host_key_fingerprint.clone()),
    )
    .await
    .map_err(|e| format!("ssh connect: {e}"))?;
    let inspect = async {
        let (ls, stderr, code) = exec_remote(&session, "docker compose ls --all --format json", 60)
            .await
            .map_err(|e| e.to_string())?;
        if code != 0 {
            return Err(format!("docker compose ls exited {code}: {stderr}"));
        }
        let dir = project_dir_from_compose_ls(&ls, crate::project_app::PLATFORM_INSTALL_DIRS)?;
        let (ps, stderr, code) = exec_remote(
            &session,
            &format!("cd {dir} && docker compose ps --all --format json"),
            60,
        )
        .await
        .map_err(|e| e.to_string())?;
        if code != 0 {
            return Err(format!("docker compose ps exited {code}: {stderr}"));
        }
        check_compose_services(&ps)?;
        Ok::<String, String>(dir)
    }
    .await;
    disconnect_ssh(session).await;
    let project_dir = inspect?;

    let outcome = crate::helpers::bake_finalize::finalize_build_box(
        &crate::helpers::bake_finalize::FinalizeContext {
            host: p.server_ip.clone(),
            port,
            user,
            private_key_pem,
            project_dir,
            stack: stack.to_string(),
            contract: p.parsed_contract,
            protected_keys: p.protected_keys,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let record = crate::helpers::bake::run_bake(
        p.connector.as_ref(),
        &p.token,
        p.target,
        stack,
        &p.version,
        true,
        "",
    )
    .await
    .map_err(|e| e.to_string())?;

    let required_env_keys = serde_json::Value::Array(
        outcome
            .required_env_keys
            .into_iter()
            .map(serde_json::Value::String)
            .collect(),
    );
    let row = db::baked_snapshot::record(
        pool,
        &record.stack,
        &record.version,
        &record.provider,
        record.image_id,
        record.healthy,
        None,
        Some(p.contract),
        Some(required_env_keys),
        p.server.server.as_deref(),
    )
    .await?;
    Ok((record.image_id, row.id))
}

#[tracing::instrument(name = "Bake status (admin)", skip_all)]
#[get("/{id}")]
pub async fn status_handler(
    _admin: web::ReqData<Arc<models::User>>, // role enforced by Casbin
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let id = uuid::Uuid::parse_str(&path.into_inner().0)
        .map_err(|_| JsonResponse::bad_request("invalid bake id"))?;
    let job = db::bake_job::fetch(pg_pool.get_ref(), id)
        .await
        .map_err(JsonResponse::internal_server_error)?
        .ok_or_else(|| JsonResponse::not_found("bake not found"))?;
    Ok(JsonResponse::build().set_item(job).ok("OK"))
}
