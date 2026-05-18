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
    pub secure: bool,
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
            secure: true,
            project_id: value.project_id,
            app_code: value.app_code,
            server_id: value.server_id,
            updated_at: value.updated_at.to_rfc3339(),
            updated_by: value.updated_by,
            source: "vault".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteSecretMetadataResponse;
    use crate::models::RemoteSecret;
    use chrono::Utc;

    #[test]
    fn remote_secret_metadata_is_explicitly_secure() {
        let metadata = RemoteSecretMetadataResponse::from(RemoteSecret {
            id: 1,
            user_id: "user-1".to_string(),
            project_id: Some(42),
            app_code: Some("web".to_string()),
            server_id: None,
            scope: "service".to_string(),
            name: "S3_SECRET".to_string(),
            vault_path: "agent/users/user-1/projects/42/apps/web/secrets/S3_SECRET".to_string(),
            updated_by: "user-1".to_string(),
            last_sync_status: "synced".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });

        assert!(metadata.secure);
        assert_eq!(metadata.source, "vault");
    }
}
