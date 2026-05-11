use std::collections::BTreeMap;
use std::sync::Arc;

use actix_web::{post, web, Responder, Result};
use sqlx::PgPool;

use crate::connectors::install_service::InstallServiceConnector;
use crate::db;
use crate::forms::cloud_firewall::{
    default_firewall_name, idempotency_key, normalize_provider, routing_key, rules_from_request,
    validate_request, CloudFirewallCredentials, CloudFirewallOperationMessage,
    CloudFirewallRequestedBy, CloudFirewallTarget, ConfigureCloudFirewallRequest,
    ConfigureCloudFirewallResponse, CLOUD_FIREWALL_PROTOCOL_VERSION,
};
use crate::helpers::{JsonResponse, MqManager};
use crate::models;

#[tracing::instrument(name = "Configure cloud firewall for server.", skip_all)]
#[post("/{id}/cloud-firewall")]
pub async fn configure(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    form: web::Json<ConfigureCloudFirewallRequest>,
    pg_pool: web::Data<PgPool>,
    mq_manager: web::Data<MqManager>,
    install_service: web::Data<Arc<dyn InstallServiceConnector>>,
) -> Result<impl Responder> {
    let server_id = path.0;
    let action = validate_request(&form)
        .map_err(|err| JsonResponse::<ConfigureCloudFirewallResponse>::build().bad_request(err))?;

    let server = db::server::fetch(pg_pool.get_ref(), server_id)
        .await
        .map_err(|err| {
            JsonResponse::<ConfigureCloudFirewallResponse>::build().internal_server_error(err)
        })
        .and_then(|server| match server {
            Some(server) if server.user_id == user.id => Ok(server),
            _ => Err(JsonResponse::<ConfigureCloudFirewallResponse>::build()
                .not_found("Server not found")),
        })?;

    let cloud_id = server.cloud_id.ok_or_else(|| {
        JsonResponse::<ConfigureCloudFirewallResponse>::build()
            .bad_request("Cloud firewall operations require a cloud-managed server")
    })?;
    let cloud = db::cloud::fetch(pg_pool.get_ref(), cloud_id)
        .await
        .map_err(|err| {
            JsonResponse::<ConfigureCloudFirewallResponse>::build().internal_server_error(err)
        })
        .and_then(|cloud| match cloud {
            Some(cloud) if cloud.user_id == user.id => Ok(cloud),
            _ => Err(JsonResponse::<ConfigureCloudFirewallResponse>::build()
                .not_found("Cloud credentials not found")),
        })?;

    let provider = normalize_provider(&cloud.provider).ok_or_else(|| {
        JsonResponse::<ConfigureCloudFirewallResponse>::build()
            .bad_request(format!("Unsupported cloud provider: {}", cloud.provider))
    })?;
    let credentials = prepare_cloud_firewall_credentials(provider, cloud)
        .map_err(|err| JsonResponse::<ConfigureCloudFirewallResponse>::build().bad_request(err))?;

    let server_public_ip = server
        .srv_ip
        .clone()
        .filter(|ip| !ip.trim().is_empty())
        .ok_or_else(|| {
            JsonResponse::<ConfigureCloudFirewallResponse>::build()
                .bad_request("Cloud firewall operations require a server public IP")
        })?;

    let managed_scope = format!("server:{}", server.id);
    let mut rules = rules_from_request(&form, managed_scope)
        .map_err(|err| JsonResponse::<ConfigureCloudFirewallResponse>::build().bad_request(err))?;
    for rule in &mut rules {
        rule.labels
            .insert("stacker.server_id".to_string(), server.id.to_string());
    }

    let target = CloudFirewallTarget {
        provider: provider.to_string(),
        cloud_id,
        server_id: server.id,
        project_id: server.project_id,
        deployment_hash: None,
        server_public_ip,
        provider_server_id: None,
        server_name: server.name.clone().or_else(|| server.server.clone()),
        region: server.region.clone(),
        zone: server.zone.clone(),
        firewall_id: None,
        firewall_name: None,
    };

    let mut provider_context = BTreeMap::new();
    provider_context.insert(
        provider.to_string(),
        serde_json::json!({ "firewall_name": default_firewall_name(&target) }),
    );

    let operation_id = format!("cfw_{}", uuid::Uuid::new_v4());
    for rule in &mut rules {
        rule.labels
            .insert("stacker.operation_id".to_string(), operation_id.clone());
    }

    let message = CloudFirewallOperationMessage {
        protocol_version: CLOUD_FIREWALL_PROTOCOL_VERSION.to_string(),
        operation_id: operation_id.clone(),
        idempotency_key: idempotency_key(server.id, &action, &rules),
        action,
        dry_run: form.dry_run,
        target,
        rules,
        credentials,
        provider_context,
        requested_by: CloudFirewallRequestedBy {
            user_id: user.id.clone(),
            user_email: Some(user.email.clone()),
        },
    };

    let response = install_service
        .configure_cloud_firewall(message, mq_manager.get_ref())
        .await
        .map_err(|err| {
            JsonResponse::<ConfigureCloudFirewallResponse>::build().internal_server_error(err)
        })?;

    let routing_key = routing_key(&response.provider).unwrap_or(response.routing_key.clone());
    Ok(JsonResponse::build()
        .set_item(ConfigureCloudFirewallResponse {
            routing_key,
            ..response
        })
        .ok("Cloud firewall operation accepted"))
}

fn prepare_cloud_firewall_credentials(
    provider: &str,
    cloud: models::Cloud,
) -> Result<CloudFirewallCredentials, String> {
    let cloud = if cloud.save_token == Some(true) {
        crate::forms::CloudForm::decode_model(cloud, true)
    } else {
        cloud
    };
    let token = non_empty_secret(cloud.cloud_token);
    let key = non_empty_secret(cloud.cloud_key);
    let secret = non_empty_secret(cloud.cloud_secret);

    if provider == "htz" && token.is_none() {
        return Err(
            "Hetzner cloud firewall operations require a valid cloud token. Please delete and re-add your Hetzner cloud credentials."
                .to_string(),
        );
    }

    Ok(CloudFirewallCredentials {
        provider: provider.to_string(),
        token,
        key,
        secret,
    })
}

fn non_empty_secret(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::CloudForm;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());
    const TEST_SECURITY_KEY: &str = "01234567890123456789012345678901";

    fn encrypted_cloud(token: &str) -> models::Cloud {
        let form = CloudForm {
            user_id: Some("user-1".to_string()),
            project_id: None,
            name: Some("prod-hetzner".to_string()),
            provider: "htz".to_string(),
            cloud_token: Some(token.to_string()),
            cloud_key: None,
            cloud_secret: None,
            save_token: Some(true),
        };

        (&form).into()
    }

    fn plaintext_cloud(token: &str) -> models::Cloud {
        models::Cloud::new(
            "user-1".to_string(),
            "prod-hetzner".to_string(),
            "htz".to_string(),
            Some(token.to_string()),
            None,
            None,
            Some(false),
        )
    }

    #[test]
    fn prepare_cloud_firewall_credentials_decodes_saved_token() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SECURITY_KEY", TEST_SECURITY_KEY);
        let cloud = encrypted_cloud("live-hcloud-token");
        let encrypted_token = cloud.cloud_token.clone();

        let credentials = prepare_cloud_firewall_credentials("htz", cloud).unwrap();

        assert_eq!(credentials.token.as_deref(), Some("live-hcloud-token"));
        assert_ne!(credentials.token, encrypted_token);
        std::env::remove_var("SECURITY_KEY");
    }

    #[test]
    fn prepare_cloud_firewall_credentials_accepts_plaintext_token() {
        let cloud = plaintext_cloud("plain-hcloud-token");

        let credentials = prepare_cloud_firewall_credentials("htz", cloud).unwrap();

        assert_eq!(credentials.token.as_deref(), Some("plain-hcloud-token"));
    }
}
