use super::InstallServiceConnector;
use crate::forms::project::Stack;
use crate::helpers::MqManager;
use crate::models;
use async_trait::async_trait;

pub struct MockInstallServiceConnector;

#[async_trait]
impl InstallServiceConnector for MockInstallServiceConnector {
    async fn deploy(
        &self,
        _user_id: String,
        _user_email: String,
        project_id: i32,
        _project: &models::Project,
        _cloud_creds: models::Cloud,
        _server: models::Server,
        _form_stack: &Stack,
        _fc: String,
        _mq_manager: &MqManager,
    ) -> Result<i32, String> {
        Ok(project_id)
    }
}
