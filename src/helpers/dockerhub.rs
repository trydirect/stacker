use serde_derive::{Deserialize, Serialize};
use serde_valid::Validate;
// use tracing_subscriber::fmt::format;
use serde_json::Value;

// #[tokio::main]
// async fn main() -> Result<(), String> {
    // tokio::select! {
    //     Ok(true) = docker_image_exist()  => {
    //         println!("first branch. image exists.");
    //     }
    //     Ok(true) = docker_image_exist()  => {
    //         println!("second branch. image exists.");
    //     }
    //     Ok(true) = docker_image_exist()  => {
    //         println!("third branch. image exists.");
    //     }
    //     else => {
    //         println!("else branch. image does not exists");
    //     }
    // };
//     Ok(())
// }


#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubToken {
    pub token: Option<String>
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct DockerHubCreds<'a>  {
    username: &'a str,
    password: &'a str
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
pub async fn login(username: &str, password: &str) -> Result<DockerHubToken, String> {
    let endpoint = "https://hub.docker.com/v2/users/login";
    let creds = DockerHubCreds { username, password };
    let token = reqwest::Client::new()
        .post(endpoint)
        .json(&creds)
        .send()
        .await
        .map_err(|err| format!("{}", err))?
        .json::<DockerHubToken>()
        .await
        .map_err(|err| format!("{}", err))?;

    Ok(token)
}


pub async fn docker_image_exists(user: &str, repo: &str, token: String) -> Result<bool, String> {
    // get repo images
    let tags_url = format!("https://hub.docker.com/v2/namespaces/{}/repositories/{}/tags",
        user, repo);

    let tags = reqwest::Client::new()
        .get(tags_url)
        .header("Accept", "application/json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| format!("{}", err))?
        .json::<TagResult>()
        // .json::<serde_json::Value>()
        .await
        .map_err(|err| format!("{}", err))?;

    // println!("tags count: {:?}", tags.count);

    if tags.count > 0 {
        // let's find at least one active tag
        let active = tags.results
            .into_iter()
            .any(|tag| tag.tag_status.contains("active") );
        // println!("is active {:?}", active);
        Ok(active)
    } else {
        Err(String::from("There was no active images found in this repository"))
    }
}
