use crate::forms;
use docker_compose_types as dctypes;
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
    pub role: forms::stack::Role,
    pub default: Option<bool>,
    pub versions: Option<Vec<forms::stack::Version>>,
    #[serde(flatten)]
    pub docker_image: forms::stack::DockerImage,
    #[serde(flatten)]
    pub requirements: forms::stack::Requirements,
    #[validate(minimum = 1)]
    pub popularity: Option<u32>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    pub avoid_render: Option<bool>,
    pub price: Option<forms::stack::Price>,
    pub icon: Option<forms::stack::Icon>,
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
    pub volumes: Option<Vec<forms::stack::Volume>>,
    #[serde(flatten)]
    pub environment: forms::stack::Environment,
    #[serde(flatten)]
    pub network: forms::stack::ServiceNetworks,
    // #[serde(flatten)]
    // pub ports: Ports,
    #[serde(rename(deserialize = "sharedPorts"))]
    #[serde(rename(serialize = "shared_ports"))]
    #[serde(alias = "shared_ports")]
    pub ports: Option<Vec<forms::stack::Port>>,
}

impl App {
    pub fn named_volumes(&self) -> IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>> { 
        let mut named_volumes = IndexMap::default();

        if self.volumes.is_none() {
            return named_volumes;
        }

        for volume in self.volumes.as_ref().unwrap() {
            if !volume.is_named_docker() {
                continue;
            }

            let k = volume.host_path.as_ref().unwrap().clone();
            let v = dctypes::MapOrEmpty::Map(dctypes::ComposeVolume {
                driver: None,
                driver_opts: Default::default(),
                external: None,
                labels: Default::default(),
                name: Some(k.clone()),
            });
            named_volumes.insert(k, v);
        }

        named_volumes
    }
}

impl AsRef<forms::stack::DockerImage> for App {
    fn as_ref(&self) -> &forms::stack::DockerImage {
        &self.docker_image
    }
}

impl TryFrom<&App> for dctypes::Service {
    type Error = String;

    fn try_from(app: &App) -> Result<Self, Self::Error> {
        let mut service = dctypes::Service {
            image: Some(app.docker_image.to_string()),
            ..Default::default()
        };

        let networks = dctypes::Networks::try_from(&app.network).unwrap_or_default();
        let ports: Vec<dctypes::Port> = match &app.ports {
            Some(ports) => {
                let mut collector = vec![];
                for port in ports {
                    collector.push(port.try_into()?);
                }
                collector
            }
            None => vec![]
        };

        let volumes: Vec<dctypes::Volumes> = match &app.volumes {
            Some(volumes) => {
                let mut collector = vec![];
                for volume in volumes {
                    collector.push(dctypes::Volumes::Advanced(volume.try_into()?));
                }

                collector
            },
            None => vec![]
        };

        let mut envs = IndexMap::new();
        for item in app.environment.environment.clone().unwrap_or_default() {
            let items = item
                .into_iter()
                .map(|(k, v)| (k, Some(dctypes::SingleValue::String(v.clone()))))
                .collect::<IndexMap<_, _>>();

            envs.extend(items);
        }

        service.networks = networks;
        service.ports = dctypes::Ports::Long(ports);
        service.restart = Some("always".to_owned());
        service.volumes = volumes;
        service.environment = dctypes::Environment::KvPair(envs);

        Ok(service)
    }
}