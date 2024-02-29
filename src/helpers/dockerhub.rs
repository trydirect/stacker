use crate::forms::project::DockerImage;
use reqwest::RequestBuilder;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubToken {
    pub token: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubCreds<'a> {
    pub(crate) username: &'a str,
    pub(crate) password: &'a str,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
struct Image {
    architecture: String,
    digest: Option<String>,
    features: Option<String>,
    last_pulled: Option<String>,
    last_pushed: Option<String>,
    os: String,
    os_features: Option<String>,
    os_version: Option<String>,
    size: i64,
    status: String,
    variant: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
struct Tag {
    content_type: String,
    creator: i64,
    digest: Option<String>,
    full_size: i64,
    id: i64,
    images: Vec<Image>,
    last_updated: String,
    last_updater: i64,
    last_updater_username: String,
    media_type: String,
    name: String,
    repository: i64,
    tag_last_pulled: Option<String>,
    tag_last_pushed: Option<String>,
    tag_status: String,
    v2: bool,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
struct TagResult {
    pub count: Option<i64>,
    next: Option<Value>,
    previous: Option<Value>,
    results: Vec<Tag>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct RepoResults {
    pub count: Option<i64>,
    pub next: Option<Value>,
    pub previous: Option<Value>,
    pub results: Vec<RepoResult>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoResult {
    pub name: String,
    pub namespace: String,
    pub repository_type: String,
    pub status: i64,
    pub status_description: String,
    pub description: String,
    pub is_private: bool,
    pub star_count: i64,
    pub pull_count: i64,
    pub last_updated: String,
    pub date_registered: String,
    pub affiliation: String,
    pub media_types: Vec<String>,
    pub content_types: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Validate)]
pub struct DockerHub<'a> {
    pub(crate) creds: DockerHubCreds<'a>,
    pub(crate) repos: String,
    pub(crate) image: String,
}

impl<'a> DockerHub<'a> {
    pub async fn login(&'a self) -> Result<DockerHubToken, String> {
        let endpoint = "https://hub.docker.com/v2/users/login";

        reqwest::Client::new()
            .post(endpoint)
            .json(&self.creds)
            .send()
            .await
            .map_err(|err| format!("{:?}", err))?
            .json::<DockerHubToken>()
            .await
            .map_err(|err| format!("{:?}", err))
    }

    pub async fn lookup_public_repo(&self) -> Result<bool, String> {
        let url = format!("https://hub.docker.com/v2/repositories/{}", &self.repos);
        tracing::debug!("Validate public repositories {:?}", url);
        let client = reqwest::Client::new()
            .get(url)
            .header("Accept", "application/json");
        let mut client = self.set_token(client).await?;
        client
            .send()
            .await
            .map_err(|err| {
                tracing::debug!("Error response {:?}", err);
                format!("{}", err)
            })?
            .json::<RepoResults>()
            .await
            .map_err(|err| format!("Error on getting results:: {}", err))
            .map(|repositories| {
                tracing::debug!("Get public image repositories response {:?}", repositories);
                if repositories.count.unwrap_or(0) > 0 {
                    // let's find at least one active tag
                    let active = repositories
                        .results
                        .into_iter()
                        .any(|repo| repo.status == 1);
                    active
                } else {
                    false
                }
            })
    }

    pub async fn lookup_private_repo(&self) -> Result<bool, String> {
        let url = format!(
            "https://hub.docker.com/v2/namespaces/{}/repositories/{}/tags",
            &self.creds.username, &self.repos
        );
        tracing::debug!("Validate image {:?}", url);
        let client = reqwest::Client::new()
            .get(url)
            .header("Accept", "application/json");
        let mut client = self.set_token(client).await?;
        client
            .send()
            .await
            .map_err(|err| format!("{}", err))?
            .json::<TagResult>()
            .await
            .map_err(|err| {
                tracing::debug!("Error response {:?}", err);
                format!("{}", err)
            })
            .map(|tags| {
                tracing::debug!("Validate private image response {:?}", tags);
                if tags.count.unwrap_or(0) > 0 {
                    // let's find at least one active tag
                    let active = tags
                        .results
                        .into_iter()
                        .any(|tag| tag.tag_status.contains("active"));
                    active
                } else {
                    false
                }
            })
    }
    pub async fn is_active(&'a self) -> Result<bool, String> {
        // if namespace/user is not set change endpoint and return a different response
        if self.creds.username.is_empty() {
            match self.lookup_public_repo().await {
                Ok(result) => Ok(result),
                Err(_e) => Ok(false),
            }
        } else {
            match self.lookup_private_repo().await {
                Ok(result) => Ok(result),
                Err(_e) => Ok(false),
            }
        }
    }

    pub async fn set_token(&self, mut client: RequestBuilder) -> Result<RequestBuilder, String> {
        if self.creds.password.is_empty() {
            tracing::debug!("Password is empty. Image should be public");
            return Ok(client);
        } else {
        }
        tracing::debug!("Password is set. Login..");
        let token = self.login().await?;

        match token.token {
            None => Ok(client),
            Some(token) => Ok(client.bearer_auth(token)),
        }
    }
}

impl<'a> From<&'a DockerImage> for DockerHub<'a> {
    fn from(image: &'a DockerImage) -> Self {
        let username = match image.dockerhub_user {
            Some(ref username) => username,
            None => "",
        };
        let password = match image.dockerhub_password {
            Some(ref password) => password,
            None => "",
        };

        DockerHub {
            creds: DockerHubCreds {
                username: username,
                password: password,
            },
            repos: image.dockerhub_name.clone().unwrap_or("".to_string()),
            image: format!("{}", image),
        }
    }
}
