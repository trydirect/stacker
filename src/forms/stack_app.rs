use crate::forms;
use crate::helpers::stack::dctypes;
use indexmap::IndexMap;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct App {
    #[serde(rename = "_etag")]
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: u32,
    #[serde(rename = "_created")]
    pub created: Option<String>,
    #[serde(rename = "_updated")]
    pub updated: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub name: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub code: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    #[serde(rename = "type")]
    pub type_field: String,
    #[serde(flatten)]
    pub role: forms::Role,
    pub default: Option<bool>,
    pub versions: Option<Vec<forms::Version>>,
    #[serde(flatten)]
    pub docker_image: forms::DockerImage,
    #[serde(flatten)]
    pub requirements: forms::Requirements,
    #[validate(minimum = 1)]
    pub popularity: Option<u32>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    pub avoid_render: Option<bool>,
    pub price: Option<forms::Price>,
    pub icon: Option<forms::Icon>,
    pub domain: Option<String>,
    pub category_id: Option<u32>,
    pub parent_app_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    pub full_description: Option<String>,
    pub description: Option<String>,
    pub plan_type: Option<String>,
    pub ansible_var: Option<String>,
    pub repo_dir: Option<String>,
    pub url_app: Option<String>,
    pub url_git: Option<String>,
    pub restart: Option<String>,
    pub volumes: Option<Vec<forms::Volume>>,
    #[serde(flatten)]
    pub environment: forms::Environment,
    #[serde(flatten)]
    pub network: forms::ServiceNetworks,
    // #[serde(flatten)]
    // pub ports: Ports,
    #[serde(rename(deserialize = "sharedPorts"))]
    #[serde(rename(serialize = "shared_ports"))]
    #[serde(alias = "shared_ports")]
    pub ports: Option<Vec<forms::Port>>,
}

impl App {
    pub fn named_volumes(&self) -> IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>> { //todo Result
        let mut named_volumes = IndexMap::default();

        let volumes = &self.volumes;
        if volumes.is_none() {
            return named_volumes;
        }

        let volumes = volumes
            .clone() //todo remove it
            .unwrap()
            .into_iter()
            .filter(|volume| is_named_docker_volume(volume.host_path.clone().unwrap().as_str()))
            .map(|volume| {
                let k = volume.host_path.clone().unwrap();
                (
                    k.clone(),
                    dctypes::MapOrEmpty::Map(dctypes::ComposeVolume {
                        driver: None,
                        driver_opts: Default::default(),
                        external: None,
                        labels: Default::default(),
                        name: Some(k.clone()),
                    }),
                    )
            })
        .collect::<IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>>>();

        named_volumes.extend(volumes);
        // tracing::debug!("Named volumes: {:?}", named_volumes);

        named_volumes
    }
}

impl AsRef<forms::DockerImage> for App {
    fn as_ref(&self) -> &forms::DockerImage {
        &self.docker_image
    }
}


fn is_named_docker_volume(volume: &str) -> bool { //todo
    // Docker named volumes typically don't contain special characters or slashes
    // They are alphanumeric and may include underscores or hyphens
    let is_alphanumeric = volume
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let does_not_contain_slash = !volume.contains('/');
    is_alphanumeric && does_not_contain_slash
}

