pub mod agent_control;
pub mod ansible_roles;
pub mod cloud;
pub mod compose;
pub mod config;
pub mod deployment;
pub mod explain;
pub mod firewall;
pub mod install_preview;
pub mod marketplace_admin;
pub mod monitoring;
pub mod pipes;
pub mod project;
pub mod proxy;
pub mod recommendations;
pub mod remote_secrets;
pub mod support;
pub mod templates;
pub mod user_service;

use crate::connectors::user_service::{ResolvedDeploymentInfo, UserServiceDeploymentResolver};
use crate::mcp::registry::ToolContext;
use crate::routes::legacy_installations::resolve_owned_deployment_by_hash;
use crate::services::{DeploymentIdentifier, DeploymentResolver};

/// Resolves a deployment identifier the way `UserServiceDeploymentResolver`
/// does, then requires the caller to own the deployment. The plain resolver
/// hands a hash back unchecked, so on its own it lets any user act on any
/// deployment whose hash they know.
pub(crate) struct OwnedDeploymentResolver<'a> {
    context: &'a ToolContext,
}

impl<'a> OwnedDeploymentResolver<'a> {
    pub(crate) fn new(context: &'a ToolContext) -> Self {
        Self { context }
    }

    pub(crate) async fn resolve(
        &self,
        identifier: &DeploymentIdentifier,
    ) -> Result<String, String> {
        let deployment_hash = self.inner().resolve(identifier).await?;
        self.ensure_owned(&deployment_hash).await?;
        Ok(deployment_hash)
    }

    pub(crate) async fn resolve_with_info(
        &self,
        identifier: &DeploymentIdentifier,
    ) -> Result<ResolvedDeploymentInfo, String> {
        let info = self.inner().resolve_with_info(identifier).await?;
        self.ensure_owned(&info.deployment_hash).await?;
        Ok(info)
    }

    fn inner(&self) -> UserServiceDeploymentResolver {
        UserServiceDeploymentResolver::from_context(
            &self.context.settings.user_service_url,
            self.context.user.access_token.as_deref(),
        )
    }

    async fn ensure_owned(&self, deployment_hash: &str) -> Result<(), String> {
        resolve_owned_deployment_by_hash(
            &self.context.pg_pool,
            self.context.settings.get_ref(),
            self.context.user.as_ref(),
            deployment_hash,
        )
        .await
        .map(|_| ())
        .map_err(|_| "Deployment not found".to_string())
    }
}

pub use agent_control::*;
pub use ansible_roles::*;
pub use cloud::*;
pub use compose::*;
pub use config::*;
pub use deployment::*;
pub use explain::*;
pub use firewall::*;
pub use install_preview::*;
pub use marketplace_admin::*;
pub use monitoring::*;
pub use pipes::*;
pub use project::*;
pub use proxy::*;
pub use recommendations::*;
pub use remote_secrets::*;
pub use support::*;
pub use templates::*;
pub use user_service::*;
