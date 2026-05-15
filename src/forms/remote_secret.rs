use crate::models::RemoteSecret;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Clone, Deserialize, Validate)]
pub struct UpsertRemoteSecretRequest {
    #[validate(min_length = 1)]
    pub value: String,
}

impl std::fmt::Debug for UpsertRemoteSecretRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpsertRemoteSecretRequest")
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSecretMetadataResponse {
    pub id: i32,
    pub scope: String,
    pub name: String,
    pub project_id: Option<i32>,
    pub app_code: Option<String>,
    pub server_id: Option<i32>,
    pub updated_at: String,
    pub updated_by: String,
    pub source: String,
}

impl From<RemoteSecret> for RemoteSecretMetadataResponse {
    fn from(value: RemoteSecret) -> Self {
        Self {
            id: value.id,
            scope: value.scope,
            name: value.name,
            project_id: value.project_id,
            app_code: value.app_code,
            server_id: value.server_id,
            updated_at: value.updated_at.to_rfc3339(),
            updated_by: value.updated_by,
            source: "vault".to_string(),
        }
    }
}
