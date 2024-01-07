use crate::forms::{stack, App, StackForm, Volume, Web};
use crate::forms;
use crate::helpers::stack::dctypes::{
    AdvancedVolumes, Compose, ComposeNetwork, ComposeNetworkSettingDetails, ComposeNetworks,
    ComposeVolume, Entrypoint, Environment, MapOrEmpty, NetworkSettings, Networks, Port, Ports,
    PublishedPort, Service, Services, SingleValue, TopLevelVolumes, Volumes,
};
use crate::models;
use indexmap::IndexMap;
use serde_yaml;
#[derive(Clone, Debug)]
struct Config {}

impl Default for Config {
    fn default() -> Self {
        Config {}
    }
}

impl Default for Port {
    fn default() -> Self {
        Port {
            target: 80,
            host_ip: None,
            published: None,
            protocol: None,
            mode: None,
        }
    }
}

/// A builder for constructing docker compose.
#[derive(Clone, Debug)]
pub struct DcBuilder {
    config: Config,
    pub(crate) stack: models::Stack,
}

impl TryInto<AdvancedVolumes> for Volume {
    type Error = String;
    fn try_into(self) -> Result<AdvancedVolumes, Self::Error> {
        let source = self.host_path.clone();
        let target = self.container_path.clone();
        tracing::debug!(
            "Volume conversion result: source: {:?} target: {:?}",
            source,
            target
        );
        Ok(AdvancedVolumes {
            source: source,
            target: target.unwrap_or("".to_string()),
            _type: "".to_string(),
            read_only: false,
            bind: None,
            volume: None,
            tmpfs: None,
        })
    }
}

impl TryInto<Port> for &stack::Port {
    type Error = String;
    fn try_into(self) -> Result<Port, Self::Error> {
        let cp = self
            .container_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|err| "Could not parse port".to_string())?;

        let hp = self
            .host_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|err| "Could not parse port".to_string())?;

        Ok(Port {
            target: cp,
            host_ip: None,
            published: Some(PublishedPort::Single(hp)),
            protocol: None,
            mode: None,
        })
    }
}

impl TryFrom<&stack::ServiceNetworks> for Networks {
    type Error = ();

    fn try_from(serviceNetworks: &stack::ServiceNetworks) -> Result<Networks, Self::Error> {
        let mut result = vec!["default_network".to_string()];
        serviceNetworks.network.as_ref().map(|networks| {
            for n in networks {
                result.push(n.to_string());
            }
        });

        Ok(Networks::Simple(result))
    }
}

fn convert_shared_ports(ports: Option<Vec<stack::Port>>) -> Result<Vec<Port>, String> {
    tracing::debug!("convert shared ports {:?}", &ports);
    let mut _ports: Vec<Port> = vec![];
    match ports {
        Some(ports) => {
            tracing::debug!("Ports >>>> {:?}", ports);
            for port in ports {}
        }
        None => {
            tracing::debug!("No ports defined by user");
            return Ok(_ports);
        }
    }

    tracing::debug!("ports {:?}", _ports);
    Ok(_ports)
}

fn is_named_docker_volume(volume: &str) -> bool {
    // Docker named volumes typically don't contain special characters or slashes
    // They are alphanumeric and may include underscores or hyphens
    let is_alphanumeric = volume
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let does_not_contain_slash = !volume.contains('/');
    is_alphanumeric && does_not_contain_slash
}

impl TryFrom<&App> for Service {
    type Error = String;

    fn try_from(app: &App) -> Result<Self, Self::Error> {
        let mut service = Service {
            image: Some(app.docker_image.to_string()),
            ..Default::default()
        };

        let networks = Networks::try_from(&app.network).unwrap_or_default();

        let ports: Vec<Port> = match &app.ports {
            Some(ports) => {
                let mut collector = vec![];
                for port in ports {
                    collector.push(port.try_into()?);
                }
                collector
            }
            None => vec![]
        };

        let volumes: Vec<AdvancedVolumes> = app //todo
            .volumes
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|x| x.try_into().unwrap())
            .collect();

        let mut envs = IndexMap::new();
        for item in app.environment.environment.clone().unwrap_or_default() { //todo
            let items = item
                .into_iter()
                .map(|(k, v)| (k, Some(SingleValue::String(v.clone()))))
                .collect::<IndexMap<_, _>>();

            envs.extend(items);
        }

        service.networks = networks;
        service.ports = Ports::Long(ports);
        service.restart = Some("always".to_owned());
        service.volumes = Volumes::Advanced(volumes);
        service.environment = Environment::KvPair(envs);

        Ok(service)
    }
}

impl Into<IndexMap<String, MapOrEmpty<NetworkSettings>>> for stack::ComposeNetworks {
    fn into(self) -> IndexMap<String, MapOrEmpty<NetworkSettings>> {
        // tracing::debug!("networks found {:?}", self.networks);
        let mut networks = vec!["default_network".to_string()];
        if self.networks.is_some() {
            networks.append(&mut self.networks.unwrap());
        }
        let networks = networks
            .into_iter()
            .map(|net| {
                (
                    net,
                    MapOrEmpty::Map(NetworkSettings {
                        attachable: false,
                        driver: None,
                        driver_opts: Default::default(),
                        enable_ipv6: false,
                        internal: false,
                        // external: None,
                        external: Some(ComposeNetwork::Bool(true)),
                        ipam: None,
                        labels: Default::default(),
                        name: Some("default".to_string()),
                    }),
                )
            })
            .collect::<IndexMap<String, _>>();
        networks
    }
}

pub fn extract_named_volumes(app: App) -> IndexMap<String, MapOrEmpty<ComposeVolume>> {
    let mut named_volumes = IndexMap::default();

    let volumes = app.volumes;
    if volumes.is_none() {
        return named_volumes;
    }

    let volumes = volumes
        .unwrap()
        .into_iter()
        .filter(|volume| is_named_docker_volume(volume.host_path.clone().unwrap().as_str()))
        .map(|volume| {
            let k = volume.host_path.clone().unwrap();
            (
                k.clone(),
                MapOrEmpty::Map(ComposeVolume {
                    driver: None,
                    driver_opts: Default::default(),
                    external: None,
                    labels: Default::default(),
                    name: Some(k.clone()),
                }),
            )
        })
        .collect::<IndexMap<String, MapOrEmpty<ComposeVolume>>>();

    named_volumes.extend(volumes);
    // tracing::debug!("Named volumes: {:?}", named_volumes);

    named_volumes
}

impl DcBuilder {
    pub fn new(stack: models::Stack) -> Self {
        DcBuilder {
            config: Config::default(),
            stack,
        }
    }

    #[tracing::instrument(name = "building stack")]
    pub fn build(&self) -> Result<String, String> {
        let mut compose_content = Compose {
            version: Some("3.8".to_string()),
            ..Default::default()
        };

        let apps = StackForm::try_from(&self.stack)?; 
        let mut services = IndexMap::new();
        let mut named_volumes = IndexMap::default();

        for app_type in &apps.custom.web {
            let service = Service::try_from(&app_type.app)?;
            services.insert(app_type.app.code.clone().to_owned(), Some(service));
            named_volumes.extend(extract_named_volumes(app_type.app.clone()));
        }

        if let Some(srvs) = apps.custom.service {
            for app_type in srvs {
                let service = Service::try_from(&app_type.app)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
                named_volumes.extend(extract_named_volumes(app_type.app.clone()));
            }
        }

        if let Some(features) = apps.custom.feature {
            for app_type in features {
                let service = Service::try_from(&app_type.app)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
                named_volumes.extend(extract_named_volumes(app_type.app.clone()));
            }
        }

        let networks = apps.custom.networks.clone();
        compose_content.networks = ComposeNetworks(networks.into());

        if !named_volumes.is_empty() {
            compose_content.volumes = TopLevelVolumes(named_volumes);
        }

        tracing::debug!("services {:?}", &services);
        compose_content.services = Services(services);

        let fname = format!("./files/{}.yml", self.stack.stack_id);
        tracing::debug!("Saving docker compose to file {:?}", fname);
        let target_file = std::path::Path::new(fname.as_str());
        let serialized = serde_yaml::to_string(&compose_content)
            .map_err(|err| format!("Failed to serialize docker-compose file: {}", err))?;

        std::fs::write(target_file, serialized.clone()).map_err(|err| format!("{}", err))?;

        Ok(serialized)
    }
}
