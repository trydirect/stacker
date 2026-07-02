use crate::configuration::get_configuration;
use crate::connectors::install_service::InstallServiceConnector;
use crate::db;
use crate::forms::cloud_firewall::{
    idempotency_key, normalize_provider, CloudFirewallAction, CloudFirewallCredentials,
    CloudFirewallOperationMessage, CloudFirewallRequestedBy, CloudFirewallRule,
    CloudFirewallTarget, CLOUD_FIREWALL_PROTOCOL_VERSION,
};
use crate::forms::firewall::{parse_public_port, FirewallRuleDirection};
use crate::forms::CloudForm;
use crate::helpers::ip::extract_ipv4_from_text;
use crate::helpers::mq_manager::MqManager;
use crate::models;
use actix_web::rt;
use actix_web::web;
use chrono::Utc;
use db::deployment;
use futures_lite::stream::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::FieldTable;
use serde_derive::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

pub struct ListenCommand {}

use serde_json::Value;

fn string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Value = serde::Deserialize::deserialize(deserializer)?;
    match v {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(serde::de::Error::custom("expected string or number")),
    }
}

fn optional_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<Value> = serde::Deserialize::deserialize(deserializer)?;
    match v {
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(Value::Null) | None => Ok(None),
        _ => Err(serde::de::Error::custom("expected string, number, or null")),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ProgressMessage {
    #[serde(deserialize_with = "string_or_number")]
    id: String,
    #[serde(default, deserialize_with = "optional_string_or_number")]
    deploy_id: Option<String>,
    #[serde(default)]
    deployment_hash: Option<String>,
    alert: i32,
    message: String,
    status: String,
    #[serde(deserialize_with = "string_or_number")]
    progress: String,
    /// Server IP returned by install service after cloud provisioning
    #[serde(default)]
    srv_ip: Option<String>,
    /// SSH port (default 22)
    #[serde(default)]
    ssh_port: Option<i32>,
}

impl ListenCommand {
    pub fn new() -> Self {
        Self {}
    }
}

fn progress_message_server_ip(msg: &ProgressMessage) -> Option<String> {
    msg.srv_ip
        .as_deref()
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| extract_ipv4_from_text(&msg.message))
}

fn extract_public_ports(metadata: &Value) -> Vec<String> {
    metadata
        .get("public_ports")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .filter(|s| !s.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn build_public_firewall_message(
    deployment: &models::Deployment,
    server: &models::Server,
    cloud: models::Cloud,
    port_strings: &[String],
    operation_id: String,
) -> Result<CloudFirewallOperationMessage, String> {
    let cloud_id = server.cloud_id.ok_or_else(|| {
        format!(
            "server {} has no cloud_id; auto-firewall skipped",
            server.id
        )
    })?;
    let server_public_ip = server
        .srv_ip
        .clone()
        .filter(|ip| !ip.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "server {} has no public IP yet; auto-firewall skipped",
                server.id
            )
        })?;
    let provider = normalize_provider(&cloud.provider).ok_or_else(|| {
        format!(
            "cloud provider '{}' does not support auto-firewall",
            cloud.provider
        )
    })?;

    let cloud = if cloud.save_token == Some(true) {
        CloudForm::decode_model(cloud, true)
    } else {
        cloud
    };
    let non_empty = |s: Option<String>| {
        s.map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let credentials = CloudFirewallCredentials {
        provider: provider.to_string(),
        token: non_empty(cloud.cloud_token),
        key: non_empty(cloud.cloud_key),
        secret: non_empty(cloud.cloud_secret),
    };
    if provider == "htz" && credentials.token.is_none() {
        return Err(format!(
            "Hetzner auto-firewall requires a valid cloud token for cloud {}",
            cloud_id
        ));
    }

    let managed_scope = format!("server:{}", server.id);
    let mut rules: Vec<CloudFirewallRule> = Vec::with_capacity(port_strings.len());
    for input in port_strings {
        let port_rule = parse_public_port(input)
            .map_err(|err| format!("invalid public_port '{}': {}", input, err))?;
        let mut rule = CloudFirewallRule::from_port_rule(
            port_rule,
            FirewallRuleDirection::Inbound,
            managed_scope.clone(),
        );
        rule.labels
            .insert("stacker.server_id".to_string(), server.id.to_string());
        rule.labels
            .insert("stacker.operation_id".to_string(), operation_id.clone());
        rule.labels.insert(
            "stacker.deployment_hash".to_string(),
            deployment.deployment_hash.clone(),
        );
        rules.push(rule);
    }
    if rules.is_empty() {
        return Err("no valid public_ports to configure".to_string());
    }

    let target = CloudFirewallTarget {
        provider: provider.to_string(),
        cloud_id,
        server_id: server.id,
        project_id: server.project_id,
        deployment_hash: Some(deployment.deployment_hash.clone()),
        server_public_ip,
        provider_server_id: None,
        server_name: server.name.clone().or_else(|| server.server.clone()),
        region: server.region.clone(),
        zone: server.zone.clone(),
        firewall_id: None,
        firewall_name: None,
    };

    let action = CloudFirewallAction::Add;
    Ok(CloudFirewallOperationMessage {
        protocol_version: CLOUD_FIREWALL_PROTOCOL_VERSION.to_string(),
        operation_id,
        idempotency_key: idempotency_key(server.id, &action, &rules),
        action,
        dry_run: false,
        target,
        rules,
        credentials,
        provider_context: BTreeMap::new(),
        requested_by: CloudFirewallRequestedBy {
            user_id: deployment.user_id.clone().unwrap_or_default(),
            user_email: None,
        },
    })
}

async fn configure_public_firewall_for_deployment(
    db_pool: &PgPool,
    install_service: &Arc<dyn InstallServiceConnector>,
    mq_manager: &MqManager,
    deployment: &models::Deployment,
) -> Result<(), String> {
    let port_strings = extract_public_ports(&deployment.metadata);
    if port_strings.is_empty() {
        return Ok(());
    }

    let servers = db::server::fetch_by_project(db_pool, deployment.project_id).await?;
    let server = servers
        .into_iter()
        .find(|s| {
            s.cloud_id.is_some()
                && s.srv_ip
                    .as_ref()
                    .map_or(false, |ip| !ip.trim().is_empty())
        })
        .ok_or_else(|| {
            format!(
                "no cloud-managed server with public IP for project {}",
                deployment.project_id
            )
        })?;

    let cloud_id = server
        .cloud_id
        .expect("cloud_id checked above during server selection");
    let cloud = db::cloud::fetch(db_pool, cloud_id)
        .await?
        .ok_or_else(|| format!("cloud {} not found for server {}", cloud_id, server.id))?;

    let operation_id = format!("cfw_{}", Uuid::new_v4());
    let message =
        build_public_firewall_message(deployment, &server, cloud, &port_strings, operation_id)?;

    install_service
        .configure_cloud_firewall(message, mq_manager)
        .await
        .map(|_| ())
}

impl crate::console::commands::CallableTrait for ListenCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        rt::System::new().block_on(async {
            let settings = get_configuration().expect("Failed to read configuration.");
            let db_pool = PgPool::connect(&settings.database.connection_string())
                .await
                .expect("Failed to connect to database.");

            let db_pool = web::Data::new(db_pool);
            let install_service_data = crate::connectors::init_install_service(&settings.connectors);
            let install_service: Arc<dyn InstallServiceConnector> =
                install_service_data.get_ref().clone();
            let queue_name = "stacker_listener";

            // Outer loop for reconnection on connection errors
            loop {
                println!("Connecting to RabbitMQ...");

                // Try to establish connection with retry
                let mq_manager =
                    match Self::connect_with_retry(&settings.amqp.connection_string()).await {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Failed to connect to RabbitMQ after retries: {}", e);
                            sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                let consumer_channel = match mq_manager
                    .consume("install_progress", queue_name, "install.progress.*.*.*")
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to create consumer: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                println!("Declare queue");
                let mut consumer = match consumer_channel
                    .basic_consume(
                        queue_name,
                        "console_listener",
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed basic_consume: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                println!("Waiting for messages ..");

                // Inner loop for processing messages
                while let Some(delivery_result) = consumer.next().await {
                    let delivery = match delivery_result {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("Consumer error (will reconnect): {}", e);
                            break; // Break inner loop to reconnect
                        }
                    };

                    let s: String = match String::from_utf8(delivery.data.to_owned()) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Invalid UTF-8 sequence: {}", e);
                            if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                                eprintln!("Failed to ack invalid message: {}", ack_err);
                            }
                            continue;
                        }
                    };

                    let statuses = vec![
                        "complete",
                        "completed",
                        "paused",
                        "failed",
                        "cancelled",
                        "in_progress",
                        "error",
                        "wait_resume",
                        "wait_start",
                        "confirmed",
                    ];

                    match serde_json::from_str::<ProgressMessage>(&s) {
                        Ok(msg) => {
                            println!("message {:?}", s);

                            if statuses.contains(&(msg.status.as_ref())) {
                                let normalized_status = if msg.status == "complete" {
                                    "completed".to_string()
                                } else {
                                    msg.status.clone()
                                };
                                // Try to find deployment by deploy_id or deployment_hash
                                let deployment_result = if let Some(ref deploy_id_str) =
                                    msg.deploy_id
                                {
                                    // Try deploy_id first (numeric ID)
                                    if let Ok(id) = deploy_id_str.parse::<i32>() {
                                        deployment::fetch(db_pool.get_ref(), id).await
                                    } else if let Some(ref hash) = msg.deployment_hash {
                                        // deploy_id might be the hash string
                                        deployment::fetch_by_deployment_hash(
                                            db_pool.get_ref(),
                                            hash,
                                        )
                                        .await
                                    } else {
                                        // Try deploy_id as hash
                                        deployment::fetch_by_deployment_hash(
                                            db_pool.get_ref(),
                                            deploy_id_str,
                                        )
                                        .await
                                    }
                                } else if let Some(ref hash) = msg.deployment_hash {
                                    // Use deployment_hash
                                    deployment::fetch_by_deployment_hash(db_pool.get_ref(), hash)
                                        .await
                                } else {
                                    // No identifier available
                                    println!("No deploy_id or deployment_hash in message");
                                    if let Err(ack_err) =
                                        delivery.ack(BasicAckOptions::default()).await
                                    {
                                        eprintln!("Failed to ack: {}", ack_err);
                                    }
                                    continue;
                                };

                                match deployment_result {
                                    Ok(Some(mut row)) => {
                                        row.status = normalized_status;
                                        row.updated_at = Utc::now();

                                        // Persist the progress message in metadata so the
                                        // status API can surface error details to CLI users.
                                        if !msg.message.is_empty() {
                                            if let Some(obj) = row.metadata.as_object_mut() {
                                                obj.insert(
                                                    "status_message".to_string(),
                                                    serde_json::Value::String(msg.message.clone()),
                                                );
                                            } else {
                                                row.metadata = serde_json::json!({
                                                    "status_message": msg.message
                                                });
                                            }
                                        }

                                        // Update server.srv_ip whenever the progress
                                        // message carries an IP from the cloud provisioner.
                                        // Previously this was gated on status == "completed",
                                        // but the IP is already known after Terraform succeeds
                                        // even when the subsequent Ansible step fails (status
                                        // "paused" / "failed").
                                        if let Some(ip) = progress_message_server_ip(&msg) {
                                            match db::server::update_srv_ip(
                                                db_pool.get_ref(),
                                                row.project_id,
                                                &ip,
                                                msg.ssh_port,
                                            )
                                            .await
                                            {
                                                Ok(s) => println!(
                                                    "Updated server {} srv_ip={} for project {}",
                                                    s.id, ip, row.project_id
                                                ),
                                                Err(e) => eprintln!(
                                                    "Failed to update srv_ip for project {}: {}",
                                                    row.project_id, e
                                                ),
                                            }
                                        }

                                        let is_completed = row.status == "completed";
                                        let row_for_firewall = row.clone();
                                        println!(
                                            "Deployment {} updated with status {}",
                                            &row.id, &row.status
                                        );
                                        if let Err(e) =
                                            deployment::update(db_pool.get_ref(), row).await
                                        {
                                            eprintln!("Failed to update deployment: {}", e);
                                        }

                                        // Auto-configure cloud firewall when a new
                                        // deployment reaches "completed" and the
                                        // project has public_ports declared.
                                        if is_completed {
                                            if let Err(e) =
                                                configure_public_firewall_for_deployment(
                                                    db_pool.get_ref(),
                                                    &install_service,
                                                    &mq_manager,
                                                    &row_for_firewall,
                                                )
                                                .await
                                            {
                                                eprintln!(
                                                    "Auto-firewall failed for deployment {}: {}",
                                                    row_for_firewall.id, e
                                                );
                                            }
                                        }
                                    }
                                    Ok(None) => println!("Deployment record was not found in db"),
                                    Err(e) => eprintln!("Failed to fetch deployment: {}", e),
                                }
                            }
                        }
                        Err(_err) => {
                            tracing::debug!("Invalid message format {:?}", _err)
                        }
                    }

                    if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                        eprintln!("Failed to ack message: {}", ack_err);
                        break; // Connection likely lost, reconnect
                    }
                }

                println!("Consumer loop ended, reconnecting in 5s...");
                sleep(Duration::from_secs(5)).await;
            }
        })
    }
}

impl ListenCommand {
    async fn connect_with_retry(connection_string: &str) -> Result<MqManager, String> {
        let max_retries = 10;
        let mut retry_delay = Duration::from_secs(1);

        for attempt in 1..=max_retries {
            println!("RabbitMQ connection attempt {}/{}", attempt, max_retries);

            match MqManager::try_new(connection_string.to_string()) {
                Ok(manager) => {
                    println!("Connected to RabbitMQ");
                    return Ok(manager);
                }
                Err(e) => {
                    eprintln!("Connection attempt {} failed: {}", attempt, e);
                    if attempt < max_retries {
                        sleep(retry_delay).await;
                        retry_delay = std::cmp::min(retry_delay * 2, Duration::from_secs(30));
                    }
                }
            }
        }

        Err(format!("Failed to connect after {} attempts", max_retries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress_message(message: &str, srv_ip: Option<&str>) -> ProgressMessage {
        ProgressMessage {
            id: "1".to_string(),
            deploy_id: Some("174".to_string()),
            deployment_hash: Some("hash".to_string()),
            alert: 0,
            message: message.to_string(),
            status: "paused".to_string(),
            progress: "90".to_string(),
            srv_ip: srv_ip.map(ToOwned::to_owned),
            ssh_port: Some(22),
        }
    }

    #[test]
    fn progress_message_server_ip_prefers_structured_srv_ip() {
        let msg = progress_message("178.104.222.170: Copy files is done", Some("203.0.113.42"));

        assert_eq!(
            progress_message_server_ip(&msg),
            Some("203.0.113.42".to_string())
        );
    }

    #[test]
    fn progress_message_server_ip_falls_back_to_message_prefix() {
        let msg = progress_message("178.104.222.170: Copy files is done", None);

        assert_eq!(
            progress_message_server_ip(&msg),
            Some("178.104.222.170".to_string())
        );
    }
}
