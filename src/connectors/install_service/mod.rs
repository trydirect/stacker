//! Install Service connector module
//!
//! Provides abstractions for delegating deployments to the external install service.

use crate::forms::project::{RegistryForm, Stack};
use crate::forms::{CloudFirewallOperationMessage, ConfigureCloudFirewallResponse};
use crate::helpers::MqManager;
use crate::models;
use async_trait::async_trait;
use serde::Serialize;

pub mod client;
pub mod init;
pub mod mock;

pub use client::InstallServiceClient;
pub use init::init;
pub use mock::MockInstallServiceConnector;

/// Payload for the post-clone-deploy Ansible setup job.
#[derive(Debug, Serialize)]
pub struct PostDeployClonePayload {
    pub deployment_hash: String,
    pub server_id: i64,
    pub public_ipv4: String,
    pub domain: String,
    pub stack: String,
    pub provider: String,
    pub user_token: String,
    pub user_email: String,
    pub installation_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_private_key: Option<String>,
}

#[async_trait]
pub trait InstallServiceConnector: Send + Sync {
    /// Deploy a project using compose file and credentials via the install service
    async fn deploy(
        &self,
        user_id: String,
        user_email: String,
        project_id: i32,
        deployment_id: i32,
        deployment_hash: String,
        project: &models::Project,
        cloud_creds: models::Cloud,
        server: models::Server,
        form_stack: &Stack,
        registry: Option<RegistryForm>,
        fc: String,
        mq_manager: &MqManager,
        server_public_key: Option<String>,
        server_private_key: Option<String>,
    ) -> Result<i32, String>;

    async fn configure_cloud_firewall(
        &self,
        message: CloudFirewallOperationMessage,
        mq_manager: &MqManager,
    ) -> Result<ConfigureCloudFirewallResponse, String>;

    /// Trigger post-clone Ansible setup (firewall, monitoring, etc.) on a
    /// server that was just cloned from a baked snapshot.
    async fn post_deploy_clone(
        &self,
        payload: PostDeployClonePayload,
        mq_manager: &MqManager,
    ) -> Result<(), String>;
}
