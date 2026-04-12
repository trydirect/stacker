use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use stacker::handoff::{
    DeploymentHandoffAgentContext, DeploymentHandoffCloudContext, DeploymentHandoffDeployment,
    DeploymentHandoffLink, DeploymentHandoffPayload, DeploymentHandoffProject,
    DeploymentHandoffServerContext,
};

#[test]
fn handoff_payload_serializes_expected_contract() {
    let payload = DeploymentHandoffPayload {
        version: "v1".to_string(),
        expires_at: Utc.with_ymd_and_hms(2026, 4, 12, 10, 0, 0).unwrap(),
        project: DeploymentHandoffProject {
            id: 42,
            name: "OpenClaw Demo".to_string(),
            identity: Some("openclaw-demo".to_string()),
        },
        deployment: DeploymentHandoffDeployment {
            id: 13828,
            hash: "JoRHfrj4".to_string(),
            target: "cloud".to_string(),
            status: "completed".to_string(),
        },
        server: Some(DeploymentHandoffServerContext {
            ip: Some("46.225.145.123".to_string()),
            ssh_user: Some("root".to_string()),
            ssh_port: Some(22),
            name: Some("openclaw-i8ntmi9e0".to_string()),
        }),
        cloud: Some(DeploymentHandoffCloudContext {
            id: Some(7),
            provider: Some("hetzner".to_string()),
            region: Some("fsn1".to_string()),
        }),
        lockfile: json!({
            "target": "cloud",
            "deployment_id": 13828,
            "project_id": 42,
            "project_name": "OpenClaw Demo"
        }),
        stacker_yml: Some("name: openclaw-demo\n".to_string()),
        agent: Some(DeploymentHandoffAgentContext {
            deployment_hash: Some("JoRHfrj4".to_string()),
            connected: Some(true),
        }),
    };

    let value = serde_json::to_value(&payload).expect("payload should serialize");

    assert_eq!(value["version"], "v1");
    assert_eq!(value["project"]["id"], 42);
    assert_eq!(value["deployment"]["target"], "cloud");
    assert_eq!(value["server"]["ip"], "46.225.145.123");
    assert_eq!(value["cloud"]["provider"], "hetzner");
    assert_eq!(value["lockfile"]["deployment_id"], 13828);
    assert_eq!(value["agent"]["connected"], true);
}

#[test]
fn handoff_link_reports_expiry_against_reference_time() {
    let issued_at = Utc.with_ymd_and_hms(2026, 4, 12, 9, 0, 0).unwrap();
    let expires_at = issued_at + Duration::minutes(15);
    let link = DeploymentHandoffLink::new("https://dev.try.direct/handoff/abc".to_string(), issued_at, expires_at);

    assert!(!link.is_expired_at(issued_at + Duration::minutes(10)));
    assert!(link.is_expired_at(issued_at + Duration::minutes(16)));
}
