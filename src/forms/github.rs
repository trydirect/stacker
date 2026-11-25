use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGithubRepoRequest {
    #[serde(rename = "projectId")]
    pub project_id: i32,
    #[serde(rename = "installationId")]
    pub installation_id: i64,
    /// `owner/name`
    pub repo: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(rename = "watchPaths", default)]
    pub watch_paths: Vec<String>,
    #[serde(rename = "autodeploy", default = "default_true")]
    pub autodeploy: bool,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGithubRepoRequest {
    pub branch: Option<String>,
    #[serde(rename = "watchPaths")]
    pub watch_paths: Option<Vec<String>>,
    #[serde(rename = "autodeploy")]
    pub autodeploy: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GithubRepoResponse {
    pub id: i32,
    #[serde(rename = "projectId")]
    pub project_id: Option<i32>,
    #[serde(rename = "installationId")]
    pub installation_id: i64,
    pub repo: String,
    pub branch: String,
    #[serde(rename = "watchPaths")]
    pub watch_paths: Vec<String>,
    #[serde(rename = "autodeploy")]
    pub autodeploy: bool,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::models::GithubRepo> for GithubRepoResponse {
    fn from(repo: crate::models::GithubRepo) -> Self {
        Self {
            id: repo.id,
            project_id: repo.project_id,
            installation_id: repo.installation_id,
            repo: repo.repo,
            branch: repo.branch,
            watch_paths: repo.watch_paths,
            autodeploy: repo.autodeploy,
            created_at: repo.created_at,
            updated_at: repo.updated_at,
        }
    }
}
