use serde_derive::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_json::Value;


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
    pub(crate) token: DockerHubToken,
    pub(crate) image: String,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Validate)]
pub(crate) struct DockerHubBuilder<'a> {
    pub(crate) creds: DockerHubCreds<'a>,
    pub(crate) repos: &'a str,
    pub(crate) token: DockerHubToken,
    pub(crate) image: &'a str,
}

impl<'a> DockerHubBuilder<'a> {

    pub async fn set_repos(&'a mut self, repo_name: &'a str) -> &'a mut Self {
        self.repos = repo_name;
        self
    }

    pub async fn set_image(&'a mut self, image: &'a str) -> &'a mut Self {
        self.image = image;
        self
    }
    pub async fn login(&'a mut self, username: &'a str, password: &'a str) -> &'a mut Self {
        let endpoint = "https://hub.docker.com/v2/users/login";

        self.creds = DockerHubCreds {
            username: username,
            password: password
        };

        if self.creds.password.is_empty() {
            tracing::debug!("DockerHub credentials were not provided, login not required/image considered public");
            return self;
        }

        let response = reqwest::Client::new()
            .post(endpoint)
            .json(&self.creds)
            .send()
            .await
            .map_err(|err| format!("{:?}", err))
            .unwrap()
            .json::<DockerHubToken>()
            .await
            .map_err(|err| format!("{:?}", err))
            .map(|token| {
                self.token = token;
            });

        tracing::debug!("Login response was: {:?}", response);
        self
    }

    pub async fn is_active(&'a self) -> Result<bool, String> {
        // get repo images
        let tags_url = format!("https://hub.docker.com/v2/namespaces/{}/repositories/{}/tags",
                               &self.creds.username, &self.repos);

        let mut client = reqwest::Client::new()
            .get(tags_url)
            .header("Accept", "application/json");

        client = match self.token.token.as_ref() {
            Some(token) => {
                if !token.is_empty() {
                    client = client.bearer_auth(token);
                }
                client
            },
            None => {
                client
            }
        };

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

}

impl<'a> DockerHub<'a>
{
    pub fn build() -> DockerHubBuilder<'a> {
        DockerHubBuilder::default()
    }
}