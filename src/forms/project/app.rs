use crate::forms;
use crate::forms::project::network::Network;
use crate::forms::project::{replace_id_with_name, DockerImage};
use docker_compose_types as dctypes;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct App {
    #[serde(rename = "_etag")]
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub etag: Option<String>,
    #[serde(rename = "_id")]
    pub id: String,
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
    pub role: forms::project::Role,
    pub default: Option<bool>,
    pub versions: Option<Vec<forms::project::Version>>,
    #[serde(flatten)]
    #[validate]
    pub docker_image: DockerImage,
    #[serde(flatten)]
    #[validate]
    pub requirements: forms::project::Requirements,
    #[validate(minimum = 1)]
    pub popularity: Option<u32>,
    pub commercial: Option<bool>,
    pub subscription: Option<Value>,
    pub autodeploy: Option<bool>,
    pub suggested: Option<bool>,
    pub dependency: Option<Value>,
    pub avoid_render: Option<bool>,
    pub price: Option<forms::project::Price>,
    pub icon: Option<forms::project::Icon>,
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
    #[validate(enumerate("always", "no", "unless-stopped", "on-failure"))]
    pub restart: String,
    pub command: Option<String>,
    pub entrypoint: Option<String>,
    /// Compose settings a stacker.yml service declares
    /// (shared-fixtures/api-contracts/stack-service-runtime-fields.json).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<crate::cli::config_parser::ComposeHealthcheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_drop: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy: Option<crate::cli::config_parser::ServiceDeploy>,
    pub volumes: Option<Vec<forms::project::Volume>>,
    #[serde(flatten)]
    pub environment: forms::project::Environment,
    #[serde(flatten)]
    pub network: forms::project::ServiceNetworks,
    #[validate]
    pub shared_ports: Option<Vec<forms::project::Port>>,
    /// Author-declared per-field policy (`config_contract` from stacker.yml).
    /// Persisted to `project_app.config_contract` during sync so the Stack
    /// Builder UI can render form fields with proper generation/validation rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_contract: Option<Value>,
}

impl App {
    #[tracing::instrument(name = "named_volumes", skip_all)]
    pub fn named_volumes(&self) -> IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>> {
        let mut named_volumes = IndexMap::default();

        if self.volumes.is_none() {
            return named_volumes;
        }

        for volume in self.volumes.as_ref().unwrap() {
            if !volume.is_named_docker_volume() {
                continue;
            }

            let k = volume.host_path.as_ref().unwrap().clone();
            let v = dctypes::MapOrEmpty::Map(volume.into());
            named_volumes.insert(k, v);
        }

        tracing::debug!("Named volumes: {:?}", named_volumes);
        named_volumes
    }

    pub(crate) fn try_into_service(
        &self,
        all_networks: &Vec<Network>,
    ) -> Result<dctypes::Service, String> {
        let mut service = dctypes::Service {
            image: Some(self.docker_image.to_string()),
            ..Default::default()
        };

        let networks = dctypes::Networks::try_from(&self.network).unwrap_or_default();

        let networks = replace_id_with_name(networks, all_networks);
        service.networks = if self.aliases.is_empty() {
            dctypes::Networks::Simple(networks)
        } else {
            let settings = dctypes::AdvancedNetworkSettings {
                aliases: self.aliases.clone(),
                ..Default::default()
            };
            dctypes::Networks::Advanced(dctypes::AdvancedNetworks(
                networks
                    .into_iter()
                    .map(|name| (name, dctypes::MapOrEmpty::Map(settings.clone())))
                    .collect(),
            ))
        };

        let ports: Vec<dctypes::Port> = match &self.shared_ports {
            Some(ports) => {
                let mut collector = vec![];
                for port in ports {
                    collector.push(port.try_into()?);
                }
                collector
            }
            None => vec![],
        };

        let volumes: Vec<dctypes::Volumes> = match &self.volumes {
            Some(volumes) => {
                let mut collector = vec![];
                for volume in volumes {
                    collector.push(dctypes::Volumes::Advanced(volume.try_into()?));
                }

                collector
            }
            None => vec![],
        };

        let mut envs = IndexMap::new();
        if let Some(item) = self.environment.environment.clone() {
            let items = item
                .into_iter()
                .map(|env_var| {
                    (
                        env_var.key,
                        Some(dctypes::SingleValue::String(env_var.value.clone())),
                    )
                })
                .collect::<IndexMap<_, _>>();

            envs.extend(items);
        }

        service.ports = dctypes::Ports::Long(ports);
        service.restart = Some(self.restart.clone());
        if let Some(cmd) = self.command.as_deref() {
            if !cmd.is_empty() {
                service.command = Some(dctypes::Command::Simple(cmd.to_owned()));
            }
        }

        if let Some(entry) = self.entrypoint.as_deref() {
            if !entry.is_empty() {
                service.entrypoint = Some(dctypes::Entrypoint::Simple(entry.to_owned()));
            }
        }
        service.volumes = volumes;
        service.environment = dctypes::Environment::KvPair(envs);
        if !self.depends_on.is_empty() {
            service.depends_on = dctypes::DependsOnOptions::Simple(self.depends_on.clone());
        }
        service.healthcheck = self.healthcheck.as_ref().map(compose_healthcheck);
        service.cap_add = self.cap_add.clone();
        service.cap_drop = self.cap_drop.clone();
        service.devices = self.devices.clone();
        service.profiles = self.profiles.clone();
        service.deploy = self.deploy.as_ref().map(compose_deploy);

        Ok(service)
    }
}

fn compose_healthcheck(hc: &crate::cli::config_parser::ComposeHealthcheck) -> dctypes::Healthcheck {
    let test = match crate::cli::compose_service_sync::healthcheck_test_value(&hc.test) {
        serde_yaml::Value::Sequence(parts) => dctypes::HealthcheckTest::Multiple(
            parts
                .iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect(),
        ),
        _ => dctypes::HealthcheckTest::Single(hc.test.clone()),
    };
    dctypes::Healthcheck {
        test: Some(test),
        interval: Some(hc.interval.clone()),
        timeout: Some(hc.timeout.clone()),
        retries: i64::from(hc.retries),
        ..Default::default()
    }
}

fn compose_deploy(deploy: &crate::cli::config_parser::ServiceDeploy) -> dctypes::Deploy {
    use crate::cli::config_parser::DeviceCount;
    let devices = deploy
        .resources
        .as_ref()
        .and_then(|r| r.reservations.as_ref())
        .map(|r| {
            r.devices
                .iter()
                .map(|d| dctypes::Device {
                    driver: d.driver.clone(),
                    // A `${VAR}` count cannot be typed here and is left out.
                    count: d.count.as_ref().and_then(|c| match c {
                        DeviceCount::All => Some(dctypes::DeviceCount::All),
                        DeviceCount::Count(n) => {
                            n.value().map(|n| dctypes::DeviceCount::Count(u64::from(n)))
                        }
                    }),
                    device_ids: (!d.device_ids.is_empty()).then(|| d.device_ids.clone()),
                    capabilities: Some(d.capabilities.clone()),
                    ..Default::default()
                })
                .collect::<Vec<_>>()
        });
    dctypes::Deploy {
        replicas: deploy
            .replicas
            .as_ref()
            .and_then(|r| r.value())
            .map(i64::from),
        resources: devices.map(|devices| dctypes::Resources {
            reservations: Some(dctypes::Limits {
                devices: Some(devices),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

impl AsRef<forms::project::DockerImage> for App {
    fn as_ref(&self) -> &forms::project::DockerImage {
        &self.docker_image
    }
}
