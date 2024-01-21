use reqwest::RequestBuilder;
use serde_derive::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_json::Value;
use crate::forms::stack::DockerImage;


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubToken {
    pub token: Option<String>
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubCreds<'a> {
    pub(crate) username: &'a str,
    pub(crate) password: &'a str
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
    v2: bool
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
struct TagResult {
    count: i64,
    next: Value,
    previous: Value,
    results: Vec<Tag>
}


#[derive(Default, Debug, Clone, PartialEq, Serialize, Validate)]
pub(crate) struct DockerHub<'a> {
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

    pub async fn is_active(&'a self) -> Result<bool, String> {

        // get repo images
        let tags_url = format!("https://hub.docker.com/v2/namespaces/{}/repositories/{}/tags",
                               &self.creds.username, &self.repos);
        let mut client = reqwest::Client::new()
            .get(tags_url)
            .header("Accept", "application/json");

        let mut client = self.set_token(client).await?;

        client
            .send()
            .await
            .map_err(|err| format!("{}", err))?
            .json::<TagResult>()
            .await
            .map_err(|err| format!("{}", err))
            .map(|tags| {
                if tags.count > 0 {
                    // let's find at least one active tag
                    let active = tags.results
                        .into_iter()
                        .any(|tag| tag.tag_status.contains("active") );
                    active
                } else {
                    false
                }
            })
    }

    pub async fn set_token(&self, mut client: RequestBuilder) -> Result<RequestBuilder, String> {

        if self.creds.password.is_empty() {
            return Ok(client);
        }
        let token = self.login().await?;

        match token.token {
            None => {
                Ok(client)
            },
            Some(token) => {
                Ok(client.bearer_auth(token))
            }
        }
    }
}

impl<'a> From <&'a DockerImage> for DockerHub<'a> {

    fn from(image: &'a DockerImage) -> Self {
        let username = match image.dockerhub_user {
            Some(ref username) => {
                username
            },
            None => ""
        };
        let password = match image.dockerhub_password {
            Some(ref password) => {
                password
            },
            None => ""
        };

        DockerHub {
            creds: DockerHubCreds {
                username: username,
                password: password
            },
            repos: image.dockerhub_name.clone().unwrap_or("".to_string()),
            image: format!("{}", image),
        }
    }
}