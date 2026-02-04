use std::collections::HashSet;

use actix_web::{get, web, Responder, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::{db, helpers::JsonResponse, models::Agent};

#[derive(Debug, Clone, Serialize, Default)]
pub struct CapabilityCommand {
    pub command_type: String,
    pub label: String,
    pub icon: String,
    pub scope: String,
    pub requires: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CapabilitiesResponse {
    pub deployment_hash: String,
    pub agent_id: Option<String>,
    pub status: String,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub system_info: Option<serde_json::Value>,
    pub capabilities: Vec<String>,
    pub commands: Vec<CapabilityCommand>,
}

struct CommandMetadata {
    command_type: &'static str,
    requires: &'static str,
    scope: &'static str,
    label: &'static str,
    icon: &'static str,
}

const COMMAND_CATALOG: &[CommandMetadata] = &[
    CommandMetadata {
        command_type: "restart",
        requires: "docker",
        scope: "container",
        label: "Restart",
        icon: "fas fa-redo",
    },
    CommandMetadata {
        command_type: "start",
        requires: "docker",
        scope: "container",
        label: "Start",
        icon: "fas fa-play",
    },
    CommandMetadata {
        command_type: "stop",
        requires: "docker",
        scope: "container",
        label: "Stop",
        icon: "fas fa-stop",
    },
    CommandMetadata {
        command_type: "pause",
        requires: "docker",
        scope: "container",
        label: "Pause",
        icon: "fas fa-pause",
    },
    CommandMetadata {
        command_type: "logs",
        requires: "logs",
        scope: "container",
        label: "Logs",
        icon: "fas fa-file-alt",
    },
    CommandMetadata {
        command_type: "rebuild",
        requires: "compose",
        scope: "deployment",
        label: "Rebuild Stack",
        icon: "fas fa-sync",
    },
    CommandMetadata {
        command_type: "backup",
        requires: "backup",
        scope: "deployment",
        label: "Backup",
        icon: "fas fa-download",
    },
];

#[tracing::instrument(name = "Get agent capabilities", skip(pg_pool))]
#[get("/{deployment_hash}/capabilities")]
pub async fn capabilities_handler(
    path: web::Path<String>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let deployment_hash = path.into_inner();

    let agent = db::agent::fetch_by_deployment_hash(pg_pool.get_ref(), &deployment_hash)
        .await
        .map_err(|err| JsonResponse::<CapabilitiesResponse>::build().internal_server_error(err))?;

    let payload = build_capabilities_payload(deployment_hash, agent);

    Ok(JsonResponse::build()
        .set_item(payload)
        .ok("Capabilities fetched successfully"))
}

fn build_capabilities_payload(
    deployment_hash: String,
    agent: Option<Agent>,
) -> CapabilitiesResponse {
    match agent {
        Some(agent) => {
            let capabilities = extract_capabilities(agent.capabilities.clone());
            let commands = filter_commands(&capabilities);

            CapabilitiesResponse {
                deployment_hash,
                agent_id: Some(agent.id.to_string()),
                status: agent.status,
                last_heartbeat: agent.last_heartbeat,
                version: agent.version,
                system_info: agent.system_info,
                capabilities,
                commands,
            }
        }
        None => CapabilitiesResponse {
            deployment_hash,
            status: "offline".to_string(),
            ..Default::default()
        },
    }
}

fn extract_capabilities(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|val| serde_json::from_value::<Vec<String>>(val).ok())
        .unwrap_or_default()
}

fn filter_commands(capabilities: &[String]) -> Vec<CapabilityCommand> {
    if capabilities.is_empty() {
        return Vec::new();
    }

    let capability_set: HashSet<&str> = capabilities.iter().map(|c| c.as_str()).collect();

    COMMAND_CATALOG
        .iter()
        .filter(|meta| capability_set.contains(meta.requires))
        .map(|meta| CapabilityCommand {
            command_type: meta.command_type.to_string(),
            label: meta.label.to_string(),
            icon: meta.icon.to_string(),
            scope: meta.scope.to_string(),
            requires: meta.requires.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_commands_by_capabilities() {
        let capabilities = vec![
            "docker".to_string(),
            "logs".to_string(),
            "irrelevant".to_string(),
        ];

        let commands = filter_commands(&capabilities);
        let command_types: HashSet<&str> =
            commands.iter().map(|c| c.command_type.as_str()).collect();

        assert!(command_types.contains("restart"));
        assert!(command_types.contains("logs"));
        assert!(!command_types.contains("backup"));
    }

    #[test]
    fn build_payload_handles_missing_agent() {
        let payload = build_capabilities_payload("hash".to_string(), None);
        assert_eq!(payload.status, "offline");
        assert!(payload.commands.is_empty());
    }

    #[test]
    fn build_payload_includes_agent_data() {
        let mut agent = Agent::new("hash".to_string());
        agent.status = "online".to_string();
        agent.capabilities = Some(serde_json::json!(["docker", "logs"]));

        let payload = build_capabilities_payload("hash".to_string(), Some(agent));
        assert_eq!(payload.status, "online");
        assert_eq!(payload.commands.len(), 5); // docker (4) + logs (1)
    }
}
