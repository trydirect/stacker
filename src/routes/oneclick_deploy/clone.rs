//! `POST /api/v1/deploy/clone` — the user-side of immutable deploy.
//!
//! Resolves a baked snapshot `image_id` for the requested stack/version, renders
//! per-user cloud-init (env + domain vhost), and clones a new Hetzner server
//! from the snapshot via `HetznerCloudConnector::create_server_from_image`.
//!
//! Protected (requires an authenticated user). Token is the TryDirect-managed
//! `HETZNER_TOKEN` (env), so users deploy on TryDirect's Hetzner account.

use std::collections::BTreeMap;
use std::sync::Arc;

use actix_web::web::Data;
use actix_web::{post, web, HttpResponse, Responder, ResponseError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::connectors::config::HetznerConfig;
use crate::connectors::hetzner::{
    HetznerCloudClient, HetznerCloudConnector, HetznerCreateServerRequest,
};
use crate::connectors::user_service::UserServiceConnector;
use crate::helpers::cloud_init::{render_user_data, BootConfig, DerivedJwtSpec};
use crate::helpers::VaultClient;
use crate::models;
use crate::models::User;
use crate::services;

#[derive(Debug, Deserialize)]
pub struct CloneRequest {
    pub stack: String,
    /// Optional. When omitted, the newest healthy snapshot for the stack is used.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_server_type")]
    pub server_type: String,
    pub domain: String,
    #[serde(default = "default_admin_email")]
    pub admin_email: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

fn default_provider() -> String {
    "hetzner".to_string()
}

fn default_region() -> String {
    "fsn1".to_string()
}

fn default_server_type() -> String {
    "cpx11".to_string()
}

fn default_admin_email() -> String {
    "admin@example.com".to_string()
}

#[derive(Debug, Serialize)]
pub struct CloneResponse {
    pub server_id: i64,
    pub public_ipv4: Option<String>,
    pub stack: String,
    pub provider: String,
    pub deployment_hash: String,
    /// Stacker's numeric project id for the project created here.
    ///
    /// The User Service stores this as `installations.stack_id`, which is what
    /// `_sync_apps_from_stacker` keys on to pull the deployment's apps across.
    /// Without it that sync returns on its first line and the Applications
    /// panel stays empty, however many containers are running. The regular
    /// install flow gets the same value from the stack mapper; one-click had
    /// no way to learn it because this response did not carry it.
    pub project_id: i32,
    /// SSH private key (PEM) for the deploy key injected into the cloned server.
    /// The user service must pass this to the install service for Ansible access.
    pub ssh_private_key: String,
    /// No longer set: the Managed server is metered daily (`managed_usage`)
    /// instead of held on the card. Kept so existing readers still parse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_id: Option<String>,
}

#[post("/clone")]
pub async fn clone_server(
    user: web::ReqData<Arc<User>>,
    form: web::Json<CloneRequest>,
    pg_pool: Data<PgPool>,
    user_service: Data<Arc<dyn UserServiceConnector>>,
) -> impl Responder {
    tracing::debug!(
        user_id = %user.id,
        stack = %form.stack,
        region = %form.region,
        "clone deploy requested"
    );

    // Require a real user token up-front. The middleware resolves the user
    // from several auth methods (agent, jwt, hmac, ...) that don't carry a
    // user-service token; clone needs one for billing and service callbacks.
    // 401 here lets the frontend redirect the user to sign in.
    if user
        .access_token
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return HttpResponse::Unauthorized().json(json!({
            "error": "Unauthorized",
            "details": "User access token is missing",
        }));
    }

    // Resolve the baked snapshot image_id.
    let snapshot = match if let Some(version) = &form.version {
        crate::db::baked_snapshot::resolve(&pg_pool, &form.stack, version, &form.provider).await
    } else {
        crate::db::baked_snapshot::resolve_latest(&pg_pool, &form.stack, &form.provider).await
    } {
        Ok(snap) => snap,
        Err(err) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "snapshot registry error",
                "details": err.to_string(),
            }))
        }
    };

    let Some(snapshot) = snapshot else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "No baked snapshot found",
            "details": format!(
                "no healthy baked snapshot for stack '{}' (provider: {}) in the registry",
                form.stack, form.provider
            ),
        }));
    };

    let Some(image_id) = snapshot.clone_image_id() else {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "Baked snapshot not healthy",
            "details": format!(
                "snapshot for '{}' v{} exists but is marked unhealthy",
                form.stack,
                snapshot.version
            ),
        }));
    };

    // ── field_policy regeneration point ───────────────────────────────────
    // A baked snapshot froze whatever value each field held when the source box
    // was baked. For `mutability: generated` fields that is exactly wrong: the
    // policy means "a fresh value per install", but every clone of this image
    // would otherwise inherit the one baked secret (shared JWT/DB creds across
    // all buyers). So we schedule a fresh value for each generated field the user
    // service did not already supply, minted *on the cloned box* at first boot by
    // the same shell generators a normal install's `generate-secrets.sh` runs —
    // one source of truth. `derived_jwt` fields are then signed on the box (HMAC)
    // against the freshly written signing key; `enum` is deferred.
    //
    // The contract is pinned to the image (baked_snapshots.config_contract); for
    // snapshots baked before that column there is nothing to regenerate (the box
    // boots with the baked values). Best-effort — a missing/invalid contract just
    // skips regeneration.
    //
    // CAVEAT: this only fixes values the app reads from env on each boot. Anything
    // the app persisted on first-run (secrets written into its DB/volume) is
    // already frozen in the snapshot and needs a post-clone rotation step or a
    // "clean" bake taken before first-run materialization.
    let mut optional = std::collections::BTreeSet::new();
    let (regen, regen_jwt) = match &snapshot.config_contract {
        Some(contract_json) => {
            match serde_json::from_value::<crate::cli::config_parser::ConfigContract>(
                contract_json.clone(),
            ) {
                Ok(contract) => {
                    optional = optional_env_keys(&contract);
                    let cmds = regen_commands(&contract, &form.env);
                    let jwt = derived_jwt_commands(&contract, &form.env);
                    if !cmds.is_empty() || !jwt.is_empty() {
                        tracing::info!(
                            stack = %form.stack,
                            fields = ?cmds.iter().map(|(k, _)| k).collect::<Vec<_>>(),
                            derived_jwt = ?jwt.iter().map(|s| &s.target_key).collect::<Vec<_>>(),
                            "will regenerate generated fields fresh on the cloned box"
                        );
                    }
                    (cmds, jwt)
                }
                Err(err) => {
                    tracing::warn!(error = %err, stack = %form.stack,
                        "config_contract on snapshot did not parse; skipping field regeneration");
                    (Vec::new(), Vec::new())
                }
            }
        }
        // Pre-column snapshots have no pinned contract; the box boots with the
        // baked values. Resolving it live by slug is a possible follow-up.
        None => (Vec::new(), Vec::new()),
    };

    // ── fail closed on an env the image cannot boot with ─────────────────
    // The bake pinned the `${VAR}` references its compose file actually needs.
    // Compose resolves an unsatisfied reference to an *empty string* and only
    // warns, and the unit's ExecStartPre ends in `|| true`, so a missing key
    // produces a running-but-broken stack rather than a visible failure. Check
    // before a server exists, so the buyer gets a refusal instead of a bill.
    //
    // Snapshots baked before `required_env_keys` carry NULL and skip this.
    let provided: std::collections::BTreeSet<String> = form
        .env
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(key, _)| key.clone())
        .chain(regen.iter().map(|(key, _)| key.clone()))
        .chain(regen_jwt.iter().map(|spec| spec.target_key.clone()))
        .collect();

    let missing = missing_required_env_keys(&snapshot.required_env_keys, &provided, &optional);
    if !missing.is_empty() {
        tracing::error!(
            stack = %form.stack,
            version = %snapshot.version,
            missing = ?missing,
            "refusing clone: baked compose references env keys the deploy would not supply"
        );
        return HttpResponse::UnprocessableEntity().json(json!({
            "error": "Incomplete environment for this snapshot",
            "details": format!(
                "the baked image for '{}' v{} references {} environment variable(s) that this \
                 deploy would not set ({}). They would resolve to empty strings at boot.",
                form.stack,
                snapshot.version,
                missing.len(),
                missing.join(", ")
            ),
        }));
    }

    // Render cloud-init with per-user env + domain. Secrets are pre-resolved (by
    // the user service) into `form.env`; `regen` mints fresh values for
    // `mutability: generated` fields on the box at first boot, reusing the same
    // shell generators a normal install's generate-secrets.sh runs; `regen_jwt`
    // signs `derived_jwt` fields afterwards with the freshly-written signing key.
    let boot = BootConfig {
        domain: form.domain.clone(),
        admin_email: form.admin_email.clone(),
        env: form.env.clone(),
        regen,
        regen_jwt,
    };
    let user_data = render_user_data(&boot);

    // TryDirect-managed Hetzner credentials.
    let htz = HetznerConfig::from_env();
    let Some(token) = htz.token.as_deref().filter(|t| !t.trim().is_empty()) else {
        return HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Hetzner not configured",
            "details": "HETZNER_TOKEN is not set on the TryDirect stacker backend",
        }));
    };

    // Generate deployment_hash and persist Project + Deployment records.
    let deployment_hash = format!("deployment_{}", Uuid::new_v4());
    let hex = &deployment_hash[deployment_hash.len() - 8..];
    let project_name = format!("oneclick-{}-{}", form.stack, hex);

    // Carry the stack composition into the one-click project so the User Service
    // can populate the Applications panel / Stack Builder / Redeploy. The baked
    // snapshot records only an image id (baked_snapshots: stack/version/provider/
    // image_id/digests), never the composition it was baked from, so we resolve it
    // from the marketplace catalog by the slug the registry keys on:
    //   baked_snapshots.stack == stack_template.slug
    //
    // Preference order, freshest first:
    //   1. The template's *source project* (stack_template_version.source_project_id
    //      -> project.request_json). This is the authoritative, up-to-date
    //      composition — already in the exact ProjectForm shape the User Service
    //      seeder consumes under `request_json.custom`, and it reflects what was
    //      actually deployed & baked (e.g. floci runs two services though the
    //      template's tech_stack lists one). It is *unsanitized*, so we run it
    //      through the same redaction the published definition gets before storing
    //      it on the buyer's project.
    //   2. Fallback: the published (already-redacted) stack_definition on the
    //      latest StackTemplateVersion, when no source project is linked. NOTE:
    //      this raw shape still needs seeder-side handling; older templates that
    //      lack a source_project_id degrade to it.
    //
    // Best-effort throughout: on any miss we fall back to an empty payload; the
    // deployment_daily billing block below still 404s unknown stacks before any
    // server is created.
    let mut source_template_id: Option<uuid::Uuid> = None;
    let mut template_version: Option<String> = None;
    let mut request_json = json!({});

    match crate::db::marketplace::get_by_slug_with_latest(&pg_pool, &form.stack).await {
        Ok((template, version)) => {
            source_template_id = Some(template.id);
            template_version = version.as_ref().map(|v| v.version.clone());

            // 1. Prefer the source project's live composition.
            let source_project =
                match crate::db::marketplace::get_source_project_id(&pg_pool, template.id).await {
                    Ok(Some(pid)) => crate::db::project::fetch(&pg_pool, pid)
                        .await
                        .ok()
                        .flatten(),
                    _ => None,
                };

            if let Some(src) = source_project {
                let mut rj = src.request_json.clone();
                crate::helpers::redact::redact_sensitive_json_values(&mut rj);
                // Re-tag provenance for this one-click deploy.
                if let Some(obj) = rj.as_object_mut() {
                    obj.insert("source".into(), json!("oneclick_clone"));
                    obj.insert("stack".into(), json!(form.stack));
                }
                request_json = rj;
            } else if let Some(v) = version {
                // 2. Fallback to the published (redacted) template definition.
                request_json = json!({
                    "source": "oneclick_clone",
                    "stack": form.stack,
                    "custom": {
                        "stack_definition": v.stack_definition,
                        "definition_format": v.definition_format,
                        "config_files": v.config_files,
                    },
                });
            }
        }
        Err(err) => {
            tracing::warn!(
                error = ?err,
                stack = %form.stack,
                "could not resolve composition for one-click project; \
                 request_json will be empty (Applications panel may show 0 services)"
            );
        }
    }

    // Marketplace access gate. Without it a priced template cloned by slug
    // deployed with neither a purchase nor a card (incident 2026-10-07).
    // AllowCardOnFile because this path settles the charge after a successful
    // deploy (the install service's completion hook). Runs before any records
    // are created so a rejected clone leaves nothing behind. A slug that is
    // not an approved marketplace template passes through unchanged.
    let gate_lookup = crate::db::marketplace::get_approved_by_slug(&pg_pool, &form.stack).await;
    match classify_gate_lookup(&form.stack, gate_lookup) {
        GateLookup::Check(gate_template) => {
            if let Err(err) = services::validate_marketplace_template_access_with_mode(
                user_service.get_ref(),
                &user,
                &gate_template,
                services::AccessMode::AllowCardOnFile,
            )
            .await
            {
                tracing::warn!(
                    stack = %form.stack,
                    error = %err,
                    "one-click clone rejected by the marketplace access gate"
                );
                return crate::services::map_access_error(err).error_response();
            }
        }
        GateLookup::Pass => {}
        GateLookup::Block(response) => return response,
    }

    // ── Managed server billing: rate and card, before any record ─────────
    // Every Quick Deploy runs on a TryDirect-managed server billed daily to
    // the buyer (shared-fixtures/api-contracts/managed-server-billing.json).
    let template = match crate::db::marketplace::get_approved_by_slug(&pg_pool, &form.stack).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            tracing::warn!(
                template_slug = %form.stack,
                "stack not registered in stack_template; refusing to deploy"
            );
            return HttpResponse::NotFound().json(json!({
                "error": "Unknown stack",
                "details": format!(
                    "stack '{}' is not registered in the marketplace catalog",
                    form.stack
                ),
            }));
        }
        Err(err) => {
            // Fail closed: without the template the rate cannot be known.
            tracing::error!(error = %err, "template lookup for billing failed");
            return HttpResponse::ServiceUnavailable().json(json!({
                "error": "billing lookup failed",
                "details": err,
            }));
        }
    };
    let server_type_rate =
        match crate::db::server_type_daily_rate::fetch(&pg_pool, &form.server_type).await {
            Ok(rate) => rate.map(|cfg| (cfg.daily_rate, cfg.monthly_cap)),
            Err(err) => {
                tracing::error!(error = %err, "server type rate lookup failed");
                return HttpResponse::ServiceUnavailable().json(json!({
                    "error": "billing lookup failed",
                    "details": err,
                }));
            }
        };
    let capability = match user_service
        .can_charge(user.access_token.as_deref().unwrap_or(""))
        .await
    {
        Ok(capability) => capability,
        Err(err) => {
            tracing::error!(error = ?err, "can_charge failed");
            return HttpResponse::ServiceUnavailable().json(json!({
                "error": "payment check failed",
                "details": err.to_string(),
            }));
        }
    };
    let billed = match managed_billing_check(
        crate::services::managed_usage::resolve_daily_rate(
            template.daily_rate,
            template.monthly_cap,
            server_type_rate,
        ),
        &capability,
        &form.server_type,
    ) {
        Ok(billed) => billed,
        Err(response) => return response,
    };

    let mut project_model = crate::models::Project::new(
        user.id.clone(),
        project_name,
        json!({"source": "oneclick_clone", "stack": form.stack}),
        request_json,
    );
    project_model.source_template_id = source_template_id;
    project_model.template_version = template_version;

    let project = match crate::db::project::insert(&pg_pool, project_model).await {
        Ok(p) => p,
        Err(err) => {
            tracing::error!(error = %err, "failed to create project for clone deploy");
            return HttpResponse::InternalServerError().json(json!({
                "error": "project creation failed",
                "details": err,
            }));
        }
    };

    // Seed project_app records so the Applications panel shows the stack's
    // services.  Only works when request_json came from a live source project
    // (which carries a ProjectForm-compatible composition).  The published-
    // template fallback stores a stack_definition blob that is not a
    // ProjectForm — parsing fails and the panel stays empty (the user can
    // add apps manually).
    if let Ok(form) =
        serde_json::from_value::<crate::forms::project::ProjectForm>(project.request_json.clone())
    {
        if let Err(err) =
            crate::project_app::sync_project_level_apps_from_form(&pg_pool, project.id, &form).await
        {
            tracing::warn!(
                error = %err,
                "failed to seed project_app records — Applications panel may be empty"
            );
        }
    }

    let mut deployment = crate::models::Deployment::new(
        project.id,
        Some(user.id.clone()),
        deployment_hash.clone(),
        "in_progress".to_string(),
        "runc".to_string(),
        json!({
            "source": "oneclick_clone",
            "stack": form.stack,
            "domain": form.domain,
            "provider": form.provider,
            "region": form.region,
        }),
    );
    deployment = match crate::db::deployment::insert(&pg_pool, deployment).await {
        Ok(d) => d,
        Err(err) => {
            tracing::error!(error = %err, "failed to create deployment for clone");
            return HttpResponse::InternalServerError().json(json!({
                "error": "deployment creation failed",
                "details": err,
            }));
        }
    };
    tracing::info!(
        deployment_id = deployment.id,
        deployment_hash = %deployment_hash,
        project_id = project.id,
        "clone deployment records created"
    );

    // Generate a per-deploy SSH keypair so Ansible can reach the server post-boot.
    let (public_key, private_key) = match VaultClient::generate_ssh_keypair() {
        Ok(pair) => pair,
        Err(err) => {
            tracing::error!(error = %err, "failed to generate SSH keypair");
            return HttpResponse::InternalServerError().json(json!({
                "error": "SSH key generation failed",
                "details": err,
            }));
        }
    };

    let client = match HetznerCloudClient::new(htz.base_url.clone()) {
        Ok(client) => client,
        Err(err) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Hetzner client init failed",
                "details": err.to_string(),
            }))
        }
    };

    let mut ssh_key_ids: Vec<i64> = Vec::new();
    match client
        .add_ssh_key(
            token,
            &format!(
                "deploy-{}-{}",
                form.stack,
                &deployment_hash[deployment_hash.len() - 8..]
            ),
            &public_key,
        )
        .await
    {
        Ok(ssh_key) => {
            ssh_key_ids.push(ssh_key.id);
        }
        Err(err) => {
            // Non-fatal: the server will be created without the key.  The user
            // can still add it manually, but post-deploy Ansible will fail.
            tracing::warn!(error = %err, "failed to register SSH key on Hetzner — post-deploy setup may fail");
        }
    }

    let request = HetznerCreateServerRequest {
        name: format!(
            "{}-{}-{}",
            form.stack,
            snapshot.version,
            &deployment_hash[11..19]
        ),
        server_type: form.server_type.clone(),
        location: form.region.clone(),
        image_id,
        ssh_key_ids,
        user_data: Some(user_data),
    };

    // Usage is recorded now and starts only when the deploy succeeds.
    if let Err(err) = crate::db::managed_usage::insert_pending(
        &pg_pool,
        &crate::db::managed_usage::NewManagedUsage {
            deployment_hash: deployment_hash.clone(),
            user_id: user.id.clone(),
            stack: form.stack.clone(),
            server_type: form.server_type.clone(),
            daily_rate_minor: billed.rate_minor,
            monthly_cap_minor: billed.cap_minor,
        },
    )
    .await
    {
        tracing::error!(error = %err, "failed to record managed usage");
        return HttpResponse::InternalServerError().json(json!({
            "error": "usage record failed",
            "details": err,
        }));
    }

    let provisioned = match client.create_server_from_image(token, request).await {
        Ok(server) => server,
        Err(err) => {
            tracing::error!(error = %err, "clone from snapshot failed");
            if let Err(close_err) =
                crate::db::managed_usage::close(&pg_pool, &deployment_hash, "deploy_failed").await
            {
                tracing::warn!(error = %close_err, "failed to close managed usage");
            }
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": "Hetzner clone failed",
                "details": err.to_string(),
            }));
        }
    };

    // Keep the VM id for the admins: nothing deletes a Quick Deploy VM
    // automatically; they delete it by hand from the alert.
    if let Err(err) =
        crate::db::managed_usage::set_server_id(&pg_pool, &deployment_hash, provisioned.id).await
    {
        tracing::warn!(error = %err, "failed to record the server id on managed usage");
    }

    // Register the server in Stacker's inventory so secrets, agent, and
    // monitoring features can resolve it.  Non-fatal: the Hetzner server is
    // already created; a missing record only breaks Stacker-managed features.
    let server_model = models::Server {
        user_id: user.id.clone(),
        project_id: project.id,
        srv_ip: provisioned.public_ipv4.clone(),
        ssh_port: Some(22),
        ssh_user: Some("root".to_string()),
        region: Some(form.region.clone()),
        server: Some(form.server_type.clone()),
        name: Some(format!(
            "{}-{}",
            form.stack,
            &deployment_hash[deployment_hash.len() - 8..]
        )),
        connection_mode: "ssh".to_string(),
        key_status: "active".to_string(),
        ..Default::default()
    };

    if let Err(err) = crate::db::server::insert(&pg_pool, server_model).await {
        tracing::warn!(
            error = %err,
            "failed to register server in inventory — secrets and agent features may not work"
        );
    }

    // Create a cloud firewall and attach it to the server so the app is
    // reachable from the internet.  Without this, Hetzner blocks everything
    // except SSH (port 22) at the provider edge.
    let firewall_rules: Vec<crate::connectors::hetzner::HetznerFirewallRule> = ["22", "80", "443"]
        .iter()
        .map(|port| crate::connectors::hetzner::HetznerFirewallRule {
            direction: "in".to_string(),
            protocol: "tcp".to_string(),
            port: port.to_string(),
            source_ips: vec!["0.0.0.0/0".to_string(), "::/0".to_string()],
        })
        .collect();

    let firewall_name = format!("frw-{}", &deployment_hash[11..19]);

    if let Err(err) = client
        .create_firewall(token, &firewall_name, firewall_rules, provisioned.id)
        .await
    {
        tracing::warn!(
            error = %err,
            "failed to create cloud firewall — ports may be unreachable"
        );
    }

    HttpResponse::Ok().json(CloneResponse {
        server_id: provisioned.id,
        public_ipv4: provisioned.public_ipv4,
        stack: form.stack.clone(),
        provider: form.provider.clone(),
        project_id: project.id,
        deployment_hash,
        ssh_private_key: private_key,
        authorization_id: None,
    })
}

/// Whether a Quick Deploy may start, and at what rate its Managed server is
/// billed. The server type must have a rate (no fallback rate), then the
/// buyer must have a verified card: the server is billed to every buyer,
/// whatever the plan (quick-deploy-managed-server-billing.feature).
fn managed_billing_check(
    rate: Option<crate::services::managed_usage::DailyRate>,
    capability: &crate::connectors::user_service::BillingCapability,
    server_type: &str,
) -> Result<crate::services::managed_usage::DailyRate, HttpResponse> {
    let Some(rate) = rate else {
        return Err(HttpResponse::UnprocessableEntity().json(json!({
            "error": "no_server_rate",
            "details": format!("server type '{server_type}' has no daily rate"),
            "server_type": server_type,
        })));
    };
    if !capability.can_charge {
        return Err(HttpResponse::PaymentRequired().json(json!({
            "error": "Payment method required",
            "details": "A verified card is required for the Managed server",
            "reason": capability.reason.as_deref().unwrap_or("no_payment_method"),
        })));
    }
    Ok(rate)
}

/// What the clone access gate does with its catalog lookup, before any
/// user-service call.
enum GateLookup<T> {
    /// An approved marketplace template: run the access check against it.
    Check(T),
    /// Not a marketplace template: nothing to gate.
    Pass,
    /// The lookup itself failed: block rather than let a possibly priced
    /// template through unchecked.
    Block(HttpResponse),
}

/// Fail closed on a lookup error. It used to log and continue, so a transient
/// catalog error let a priced template clone with no purchase or card - the
/// same fail-open hole the user service's gate was closed for (2026-10-07).
fn classify_gate_lookup<T>(stack: &str, lookup: Result<Option<T>, String>) -> GateLookup<T> {
    match lookup {
        Ok(Some(template)) => GateLookup::Check(template),
        Ok(None) => GateLookup::Pass,
        Err(err) => {
            tracing::error!(
                error = %err,
                stack = %stack,
                "marketplace access gate could not resolve the template; blocking the clone"
            );
            GateLookup::Block(HttpResponse::ServiceUnavailable().json(json!({
                "error": "Marketplace catalog unavailable",
                "details": "Could not verify this template's billing. Please retry shortly.",
            })))
        }
    }
}

/// The pinned `${VAR}` references the buyer's environment would leave unset.
///
/// `required` is `baked_snapshots.required_env_keys` — a JSON array recorded at
/// bake time. `None` (a snapshot baked before that column) yields no misses, so
/// existing images keep deploying unchanged rather than becoming undeployable.
///
/// A key the snapshot's contract marks `required: false` may stay empty: the
/// build box ran with it empty too (prod 2026-10-19, local-ai-packaged's
/// OPENAI_API_KEY and Supabase API keys).
fn missing_required_env_keys(
    required: &Option<serde_json::Value>,
    provided: &std::collections::BTreeSet<String>,
    optional: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    let Some(serde_json::Value::Array(keys)) = required else {
        return Vec::new();
    };

    keys.iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|key| !provided.contains(*key) && !optional.contains(*key))
        .map(str::to_string)
        .collect()
}

/// The fields a contract lets the buyer leave empty (`required: false`).
fn optional_env_keys(
    contract: &crate::cli::config_parser::ConfigContract,
) -> std::collections::BTreeSet<String> {
    contract
        .services
        .values()
        .flat_map(|target| target.fields.iter())
        .filter(|(_, policy)| !policy.required)
        .map(|(key, _)| key.clone())
        .collect()
}

/// The generated fields whose fresh value must be minted on the cloned box,
/// paired with the canonical shell generator for each. Reuses the *single*
/// source of truth for the type→generator mapping
/// (`console::…::init::generator_shell_expression`) rather than duplicating it:
/// generation stays on the box, exactly as a normal install's
/// `generate-secrets.sh` does. `enum` is deferred here; `derived_jwt` is handled
/// separately by [`derived_jwt_commands`] (it needs its signing field first).
///
/// A key already supplied (non-empty) in `already_set` is skipped — the user
/// service may have resolved it deliberately; we only fill the gap the frozen
/// snapshot leaves.
fn regen_commands(
    contract: &crate::cli::config_parser::ConfigContract,
    already_set: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    use crate::console::commands::cli::init::{
        flatten_generated_field_policies, generator_shell_expression,
    };

    let mut cmds: Vec<(String, String)> = flatten_generated_field_policies(contract)
        .into_iter()
        .filter(|(key, _)| already_set.get(key).map(|v| v.is_empty()).unwrap_or(true))
        .filter_map(|(key, policy)| generator_shell_expression(&policy).map(|expr| (key, expr)))
        .collect();
    cmds.sort();
    cmds
}

/// Build the `derived_jwt` specs to sign on the box after `regen_commands` runs.
///
/// The header and claims are known at build time, so we precompute their
/// base64url here and let the box do only the HMAC over `header.payload` with
/// the runtime value of the signing field (which `regen_commands` will have
/// written to `/etc/stacker/env`). Only HMAC algorithms (HS256/384/512) are
/// supported — they can be signed with the shared secret already on the box;
/// asymmetric algs would need a private key we don't ship. A field already
/// supplied in `already_set` is skipped.
fn derived_jwt_commands(
    contract: &crate::cli::config_parser::ConfigContract,
    already_set: &BTreeMap<String, String>,
) -> Vec<DerivedJwtSpec> {
    use crate::cli::config_parser::{FieldType, Mutability};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    let mut specs = Vec::new();
    for service in contract.services.values() {
        for (key, policy) in &service.fields {
            if policy.mutability != Mutability::Generated
                || policy.type_spec != Some(FieldType::DerivedJwt)
            {
                continue;
            }
            if already_set.get(key).map(|v| !v.is_empty()).unwrap_or(false) {
                continue;
            }
            let (Some(signing_ref), Some(claims), Some(alg)) = (
                policy.signing_key.as_deref(),
                policy.claims.as_ref(),
                policy.alg.as_deref(),
            ) else {
                continue;
            };
            let openssl_dgst = match alg {
                "HS256" => "sha256",
                "HS384" => "sha384",
                "HS512" => "sha512",
                _ => continue,
            }
            .to_string();
            // signing_key is "service.FIELD_NAME"; the env var is FIELD_NAME.
            let signing_env_key = signing_ref
                .rsplit('.')
                .next()
                .unwrap_or(signing_ref)
                .to_string();
            let header = format!("{{\"alg\":\"{alg}\",\"typ\":\"JWT\"}}");
            let claims_json = serde_json::to_string(claims).unwrap_or_else(|_| "{}".to_string());
            specs.push(DerivedJwtSpec {
                target_key: key.clone(),
                signing_env_key,
                header_b64: URL_SAFE_NO_PAD.encode(header.as_bytes()),
                payload_b64: URL_SAFE_NO_PAD.encode(claims_json.as_bytes()),
                openssl_dgst,
            });
        }
    }
    specs.sort_by(|a, b| a.target_key.cmp(&b.target_key));
    specs
}

#[cfg(test)]
mod regen_tests {
    use super::{
        derived_jwt_commands, missing_required_env_keys, optional_env_keys, regen_commands,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    /// A `generated` field with no installer-supplied value gets a regen command
    /// reusing the canonical shell generator; a `fixed` field and an
    /// already-supplied value are left alone.
    #[test]
    fn missing_required_env_keys_reports_only_unsatisfied_ones() {
        let required = Some(json!(["ALPHA", "BETA", "GAMMA"]));
        let provided: std::collections::BTreeSet<String> =
            ["ALPHA".to_string(), "GAMMA".to_string()]
                .into_iter()
                .collect();

        assert_eq!(
            missing_required_env_keys(&required, &provided, &Default::default()),
            vec!["BETA".to_string()]
        );
    }

    /// prod 2026-10-19 (local-ai-packaged): OPENAI_API_KEY and the Supabase
    /// API keys are `provided, required: false` and were empty on the build
    /// box, but the bake recorded them in required_env_keys, so every clone
    /// without them was refused - a Quick Deploy was impossible unless the
    /// buyer typed something into optional fields.
    #[test]
    fn a_key_the_contract_marks_optional_may_stay_empty() {
        let required = Some(json!([
            "OPENAI_API_KEY",
            "SUPABASE_SECRET_KEY",
            "POSTGRES_PASSWORD"
        ]));
        let provided: std::collections::BTreeSet<String> =
            ["POSTGRES_PASSWORD".to_string()].into_iter().collect();
        let optional: std::collections::BTreeSet<String> = [
            "OPENAI_API_KEY".to_string(),
            "SUPABASE_SECRET_KEY".to_string(),
        ]
        .into_iter()
        .collect();

        assert!(missing_required_env_keys(&required, &provided, &optional).is_empty());
    }

    #[test]
    fn a_required_key_is_still_reported_next_to_optional_ones() {
        let required = Some(json!(["OPENAI_API_KEY", "POSTGRES_PASSWORD"]));
        let optional: std::collections::BTreeSet<String> =
            ["OPENAI_API_KEY".to_string()].into_iter().collect();

        assert_eq!(
            missing_required_env_keys(&required, &Default::default(), &optional),
            vec!["POSTGRES_PASSWORD".to_string()]
        );
    }

    #[test]
    fn optional_keys_come_from_the_snapshot_contract() {
        let contract: crate::cli::config_parser::ConfigContract = serde_json::from_value(json!({
            "services": {"studio": {"fields": {
                "OPENAI_API_KEY": {"mutability": "provided", "required": false},
                "POSTGRES_PASSWORD": {"mutability": "generated", "type": "alphanumeric", "min_length": 32}
            }}}
        }))
        .expect("contract parses");
        let optional = optional_env_keys(&contract);
        assert!(optional.contains("OPENAI_API_KEY"));
        assert!(!optional.contains("POSTGRES_PASSWORD"));
    }

    /// Snapshots baked before the column must stay deployable.
    #[test]
    fn missing_required_env_keys_is_empty_for_legacy_snapshots() {
        assert!(
            missing_required_env_keys(&None, &Default::default(), &Default::default()).is_empty()
        );
    }

    #[test]
    fn missing_required_env_keys_is_empty_when_all_supplied() {
        let required = Some(json!(["POSTGRES_PASSWORD"]));
        let provided: std::collections::BTreeSet<String> =
            ["POSTGRES_PASSWORD".to_string()].into_iter().collect();
        assert!(missing_required_env_keys(&required, &provided, &Default::default()).is_empty());
    }

    #[test]
    fn regen_commands_only_for_unset_generated_fields() {
        let contract: crate::cli::config_parser::ConfigContract = serde_json::from_value(json!({
            "services": {
                "web": {
                    "fields": {
                        "JWT_SECRET": { "mutability": "generated", "type": "hex", "length": 32 },
                        "API_KEY":    { "mutability": "generated", "type": "alphanumeric" },
                        "LOG_LEVEL":  { "mutability": "fixed" }
                    }
                }
            }
        }))
        .expect("contract parses");

        // API_KEY already supplied by the installer → must be skipped.
        let mut already = BTreeMap::new();
        already.insert("API_KEY".to_string(), "supplied".to_string());

        let cmds = regen_commands(&contract, &already);
        let keys: Vec<&str> = cmds.iter().map(|(k, _)| k.as_str()).collect();

        assert_eq!(
            keys,
            vec!["JWT_SECRET"],
            "only the unset generated field regenerates"
        );
        // And it reuses the openssl-based hex generator, not a bespoke one.
        assert!(cmds[0].1.contains("openssl rand -hex"));
    }

    /// A `derived_jwt` field produces a spec that: targets the right signing env
    /// var, precomputes the correct header/claims (base64url), and picks the
    /// matching openssl digest — and is NOT emitted as a plain shell generator.
    #[test]
    fn derived_jwt_spec_is_built_from_policy() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

        let contract: crate::cli::config_parser::ConfigContract = serde_json::from_value(json!({
            "services": {
                "auth": {
                    "fields": {
                        "JWT_SECRET": { "mutability": "generated", "type": "hex", "length": 32 },
                        "ANON_KEY": {
                            "mutability": "generated",
                            "type": "derived_jwt",
                            "signing_key": "auth.JWT_SECRET",
                            "claims": { "role": "anon", "iss": "supabase" },
                            "alg": "HS256"
                        }
                    }
                }
            }
        }))
        .expect("contract parses");
        let empty = BTreeMap::new();

        // derived_jwt is not emitted as a shell generator...
        let shell = regen_commands(&contract, &empty);
        assert!(
            shell.iter().all(|(k, _)| k != "ANON_KEY"),
            "derived_jwt must not go through the plain shell-generator path"
        );

        // ...it's a dedicated jwt spec.
        let jwt = derived_jwt_commands(&contract, &empty);
        assert_eq!(jwt.len(), 1);
        let spec = &jwt[0];
        assert_eq!(spec.target_key, "ANON_KEY");
        assert_eq!(spec.signing_env_key, "JWT_SECRET"); // resolved from "auth.JWT_SECRET"
        assert_eq!(spec.openssl_dgst, "sha256");

        let header = URL_SAFE_NO_PAD
            .decode(&spec.header_b64)
            .expect("header b64url");
        assert_eq!(header, br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD
            .decode(&spec.payload_b64)
            .expect("payload b64url");
        let claims: serde_json::Value = serde_json::from_slice(&payload).expect("claims json");
        assert_eq!(claims["role"], "anon");
        assert_eq!(claims["iss"], "supabase");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;
    use actix_web::web;
    use actix_web::HttpMessage;

    fn capability(can_charge: bool) -> crate::connectors::user_service::BillingCapability {
        crate::connectors::user_service::BillingCapability {
            can_charge,
            reason: (!can_charge).then(|| "no_payment_method".to_string()),
        }
    }

    const CPX32_RATE: crate::services::managed_usage::DailyRate =
        crate::services::managed_usage::DailyRate {
            rate_minor: 86,
            cap_minor: 2600,
        };

    #[actix_web::test]
    async fn a_server_type_without_a_rate_is_refused_before_the_card() {
        let resp = managed_billing_check(None, &capability(false), "ccx63").unwrap_err();
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
        );
        let body: serde_json::Value =
            serde_json::from_slice(&actix_web::body::to_bytes(resp.into_body()).await.unwrap())
                .unwrap();
        assert_eq!(body["error"], "no_server_rate");
        assert_eq!(body["server_type"], "ccx63");
    }

    #[actix_web::test]
    async fn a_buyer_without_a_card_is_asked_for_one() {
        let resp =
            managed_billing_check(Some(CPX32_RATE), &capability(false), "cpx32").unwrap_err();
        assert_eq!(resp.status(), actix_web::http::StatusCode::PAYMENT_REQUIRED);
        let body: serde_json::Value =
            serde_json::from_slice(&actix_web::body::to_bytes(resp.into_body()).await.unwrap())
                .unwrap();
        assert_eq!(body["reason"], "no_payment_method");
    }

    #[actix_web::test]
    async fn a_buyer_with_a_card_is_billed_at_the_resolved_rate() {
        assert_eq!(
            managed_billing_check(Some(CPX32_RATE), &capability(true), "cpx32").unwrap(),
            CPX32_RATE
        );
    }

    /// The User Service stores this as `installations.stack_id`; without it
    /// `_sync_apps_from_stacker` returns on its first line and the
    /// Applications panel is empty for every one-click deployment.
    // `use actix_web::test` shadows the built-in attribute in this module.
    #[actix_web::test]
    async fn clone_response_carries_the_project_id() {
        let resp = CloneResponse {
            server_id: 42,
            public_ipv4: Some("203.0.113.10".to_string()),
            stack: "floci".to_string(),
            provider: "hetzner".to_string(),
            project_id: 194,
            deployment_hash: "deployment_abc".to_string(),
            ssh_private_key: "<key>".to_string(),
            authorization_id: None,
        };

        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["project_id"], 194);
    }

    fn test_user(token: Option<String>) -> Arc<User> {
        Arc::new(User {
            id: "test-user-1".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            email: "test@example.com".to_string(),
            role: "user".to_string(),
            email_confirmed: true,
            mfa_verified: true,
            access_token: token,
        })
    }

    fn valid_payload() -> serde_json::Value {
        serde_json::json!({
            "stack": "wordpress",
            "provider": "hetzner",
            "region": "fsn1",
            "server_type": "cpx11",
            "domain": "example.com",
            "admin_email": "admin@example.com",
            "env": {},
        })
    }

    async fn call_clone(user: Arc<User>) -> actix_web::http::StatusCode {
        let pg_pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/stacker_test")
            .expect("lazy pool");
        let user_service: Arc<dyn crate::connectors::user_service::UserServiceConnector> =
            Arc::new(crate::connectors::user_service::mock::MockUserServiceConnector);

        let app = test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(pg_pool))
                .app_data(web::Data::new(user_service))
                .service(clone_server),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/clone")
            .set_json(valid_payload())
            .insert_header(("Authorization", "Bearer dummy-token"))
            .to_request();
        req.extensions_mut().insert(Arc::clone(&user));

        let resp = test::call_service(&app, req).await;
        resp.status()
    }

    #[actix_web::test]
    async fn clone_without_access_token_returns_401() {
        let status = call_clone(test_user(None)).await;
        assert_eq!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn clone_with_blank_access_token_returns_401() {
        let status = call_clone(test_user(Some("   ".to_string()))).await;
        assert_eq!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn gate_lookup_error_blocks_the_clone_with_503() {
        match classify_gate_lookup::<()>("paid-kit", Err("db timeout".to_string())) {
            GateLookup::Block(resp) => {
                assert_eq!(
                    resp.status(),
                    actix_web::http::StatusCode::SERVICE_UNAVAILABLE
                )
            }
            _ => panic!("a failed catalog lookup must block the clone, not continue"),
        }
    }

    #[actix_web::test]
    async fn gate_lookup_miss_passes_non_marketplace_stacks() {
        assert!(matches!(
            classify_gate_lookup::<()>("my-repo", Ok(None)),
            GateLookup::Pass
        ));
    }

    #[actix_web::test]
    async fn gate_lookup_hit_runs_the_access_check() {
        assert!(matches!(
            classify_gate_lookup("paid-kit", Ok(Some(7u8))),
            GateLookup::Check(7)
        ));
    }

    #[actix_web::test]
    async fn clone_with_access_token_passes_auth_guard() {
        // The guard should pass; the handler then proceeds (and fails later
        // on snapshot/DB/network, not on auth).
        let status = call_clone(test_user(Some("valid-token".to_string()))).await;
        assert_ne!(status, actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
