use super::InstallServiceConnector;
use crate::forms::project::Stack;
use crate::helpers::{compressor::compress, MqManager};
use crate::models;
use async_trait::async_trait;
use uuid::Uuid;

/// Real implementation that publishes deployment requests through RabbitMQ
pub struct InstallServiceClient;

#[async_trait]
impl InstallServiceConnector for InstallServiceClient {
    async fn deploy(
        &self,
        user_id: String,
        user_email: String,
        project_id: i32,
        project: &models::Project,
        cloud_creds: models::Cloud,
        server: models::Server,
        form_stack: &Stack,
        fc: String,
        mq_manager: &MqManager,
    ) -> Result<i32, String> {
        // Build payload for the install service
        let mut payload = crate::forms::project::Payload::try_from(project)
            .map_err(|err| format!("Failed to build payload: {}", err))?;

        payload.server = Some(server.into());
        payload.cloud = Some(cloud_creds.into());
        payload.stack = form_stack.clone().into();
        payload.user_token = Some(user_id);
        payload.user_email = Some(user_email);
        payload.docker_compose = Some(compress(fc.as_str()));

        // Prepare deployment metadata
        let json_request = project.metadata.clone();
        let deployment_hash = format!("deployment_{}", Uuid::new_v4());
        let _deployment = models::Deployment::new(
            project.id,
            payload.user_token.clone(),
            deployment_hash.clone(),
            String::from("pending"),
            json_request,
        );

        let _deployment_id = Uuid::new_v4();

        tracing::debug!("Send project data: {:?}", payload);

        let provider = payload
            .cloud
            .as_ref()
            .map(|form| if form.provider.contains("own") { "own" } else { "tfa" })
            .unwrap_or("tfa")
            .to_string();

        let routing_key = format!("install.start.{}.all.all", provider);
        tracing::debug!("Route: {:?}", routing_key);

        mq_manager
            .publish("install".to_string(), routing_key, &payload)
            .await
            .map_err(|err| format!("Failed to publish to MQ: {}", err))?;

        Ok(project_id)
    }
}
