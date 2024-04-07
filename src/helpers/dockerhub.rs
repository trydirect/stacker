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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct OfficialRepoResults {
    pub count: Option<i64>,
    pub next: Option<Value>,
    pub previous: Option<Value>,
    pub results: Vec<OfficialRepoResult>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoResult {
    pub name: String,
    pub namespace: Option<String>,
    pub repository_type: Option<String>,
    pub status: Option<i64>,
    pub status_description: Option<String>,
    pub description: Option<String>,
    pub is_private: Option<bool>,
    pub star_count: Option<i64>,
    pub pull_count: Option<i64>,
    pub last_updated: String,
    pub date_registered: Option<String>,
    pub affiliation: Option<String>,
    pub media_types: Option<Vec<String>>,
    pub content_types: Option<Vec<String>>,
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfficialRepoResult {
    pub images: Vec<Image>,
    pub last_updated: String,
    pub last_updater: i64,
    pub content_type: String,
    pub creator: i64,
    pub digest: Option<String>,
    pub full_size: i64,
    pub id: i64,
    pub last_updater_username: String,
    pub media_type: String,
    pub name: String,
    pub repository: i64,
    pub tag_last_pulled: Option<String>,
    pub tag_last_pushed: Option<String>,
    pub tag_status: String,
    pub v2: bool,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Validate)]
pub struct DockerHub<'a> {
    pub(crate) creds: DockerHubCreds<'a>,
    pub(crate) repos: String,
    pub(crate) image: String,
    pub(crate) tag: Option<String>,
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

    pub async fn lookup_public_repos(&'a self) -> Result<bool, String> {

        let url = format!("https://hub.docker.com/v2/repositories/{}", self.repos);
        tracing::debug!("Validate public repository {:?}", &url);
        let client = reqwest::Client::new()
            .get(&url)
            .header("Accept", "application/json");
        let client = self.set_token(client).await?;
        client
            .send()
            .await
            .map_err(|err| {
                let msg = format!("🟥Error response {:?}", err);
                tracing::debug!(msg);
                msg
            })?
            .json::<RepoResults>()
            .await
            .map_err(|err| {
                let msg = format!("🟥Error on getting results:: {} url: {}", &err, &url);
                tracing::error!(msg);
                msg
            })
            .map(|repositories| {
                tracing::debug!("Get public image repo {:?} response {:?}", &url, repositories);
                if repositories.count.unwrap_or(0) > 0 {
                    // let's find at least one active tag
                    let active = repositories
                        .results
                        .into_iter()
                        .any(|repo| repo.status == Some(1));
                    tracing::debug!("✅ Image is active. url: {:?}", &url);
                    active
                } else {
                    tracing::debug!("🟥 Image tag is not active, url: {:?}", &url);
                    false
                }
            })
    }

    pub async fn lookup_official_repos(&'a self) -> Result<bool, String> {
        // search in official library repositories
        let url = format!("https://hub.docker.com/v2/repositories/library/{}/tags", self.repos);
        return self.lookup(&url).await;
    }

    pub async fn lookup(&'a self, url: &String) -> Result<bool, String> {
        tracing::debug!("Search official repos {:?}", url);
        let client = reqwest::Client::new()
            .get(url)
            .header("Accept", "application/json");
        let client = self.set_token(client).await?;
        client
            .send()
            .await
            .map_err(|err| format!("🟥{}", err))?
            .json::<OfficialRepoResults>()
            .await
            .map_err(|err| {
                tracing::debug!("🟥Error response {:?}", err);
                format!("{}", err)
            })
            .map(|tags| {
                tracing::debug!("Validate official image response {:?}", tags);
                if tags.count.unwrap_or(0) > 0 {
                    // let's find at least one active tag
                    let result = tags
                        .results
                        .into_iter()
                        .any(|tag| {
                            tracing::debug!("official: {:?}", tag);
                            if "active".to_string() == tag.tag_status {
                                tracing::debug!("✅ Image is active");
                                true
                            } else {
                                false
                            }
                        });
                    tracing::debug!("✅ result is {:?}", result);
                    result
                } else {
                    tracing::debug!("🟥 Image tag is not active");
                    false
                }
            })
    }

    pub async fn lookup_private_repo(&'a self) -> Result<bool, String> {
        let url = format!(
            "https://hub.docker.com/v2/namespaces/{}/repositories/{}/tags",
            &self.creds.username, &self.repos
        );
        tracing::debug!("Validate image {:?}", url);
        let client = reqwest::Client::new()
            .get(url)
            .header("Accept", "application/json");
        let client = self.set_token(client).await?;
        client
            .send()
            .await
            .map_err(|err| format!("🟥{}", err))?
            .json::<TagResult>()
            .await
            .map_err(|err| {
                tracing::debug!("🟥Error response {:?}", err);
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
                    tracing::debug!("✅ Image is active");
                    return active;
                } else {
                    tracing::debug!("🟥 Image tag is not active");
                    false
                }
            })
    }
    pub async fn is_active(&'a mut self) -> Result<bool, String> {
        // if namespace/user is not set change endpoint and return a different response

        // let repo = self.repos.clone();
        // if self.repos.contains(':') {
        //         let _repo = repo.split(':')
        //         .collect::<Vec<&str>>();
        //     self.repos = _repo.first().expect("split error").to_string();
        //     self.tag = Some(_repo.last().expect("split error").to_string());
        // }

        // let n = self.repos.split(':').collect::<Vec<&str>>();
        // if let Some(name: &str) == n.first() {
        //     self.repos = name.to_string();
        // }
        // if let Some(name: &str) == n.last() {
        //     self.tag = name.to_string();
        // }

        if self.creds.username.is_empty() {

            if Ok(true) == self.lookup_official_repos().await {
                tracing::debug!("official: true");
                return Ok(true);
            } else {
                tracing::debug!("official: false");
            };

            if Ok(true) == self.lookup_public_repos().await {
                tracing::debug!("public: true");
                return Ok(true);
            };

            Ok(false)

        } else {

            if Ok(true) == self.lookup_private_repo().await {
                tracing::debug!("private: true");
                return Ok(true);
            };

            Ok(false)
        }
    }

    pub async fn set_token(&'a self, client: RequestBuilder) -> Result<RequestBuilder, String> {
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

        let name = image.dockerhub_name.clone().unwrap_or("".to_string());
        let mut tag = "".to_string();

        if name.contains(':') {
            let parts = name
                .split(':')
                .collect::<Vec<&str>>();
            let name = parts.first().expect("Could not split").to_string();
            tag = parts.last().expect("Could not split").to_string();
        }

        DockerHub {
            creds: DockerHubCreds {
                username: username,
                password: password,
            },
            repos: name,
            image: format!("{}", image),
            tag: Some(tag),
        }
    }
}
