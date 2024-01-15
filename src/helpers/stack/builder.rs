use crate::forms;
use docker_compose_types::{
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

/// A builder for constructing docker compose.
#[derive(Clone, Debug)]
pub struct DcBuilder {
    config: Config,
    pub(crate) stack: models::Stack,
}

impl TryInto<AdvancedVolumes> for forms::stack::Volume {
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

impl TryInto<Port> for &forms::stack::Port {
    type Error = String;
    fn try_into(self) -> Result<Port, Self::Error> {
        let cp = self
            .container_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|_| "Could not parse port".to_string())?;

        let hp = self
            .host_port
            .as_ref()
            .map_or(Ok(0u16), |s| s.parse::<u16>())
            .map_err(|_| "Could not parse port".to_string())?;

        Ok(Port {
            target: cp,
            host_ip: None,
            published: Some(PublishedPort::Single(hp)),
            protocol: None,
            mode: None,
        })
    }
}

impl TryFrom<&forms::stack::ServiceNetworks> for Networks {
    type Error = ();

    fn try_from(service_networks: &forms::stack::ServiceNetworks) -> Result<Networks, Self::Error> {
        let mut result = vec!["default_network".to_string()];
        service_networks.network.as_ref().map(|networks| {
            for n in networks {
                result.push(n.to_string());
            }
        });

        Ok(Networks::Simple(result))
    }
}


impl TryFrom<&forms::stack::App> for Service {
    type Error = String;

    fn try_from(app: &forms::stack::App) -> Result<Self, Self::Error> {
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

        let volumes: Vec<Volumes> = app 
            .volumes
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|x| Volumes::Advanced(x.try_into().unwrap())) //todo
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
        service.volumes = volumes;
        service.environment = Environment::KvPair(envs);

        Ok(service)
    }
}

impl Into<IndexMap<String, MapOrEmpty<NetworkSettings>>> for forms::stack::ComposeNetworks {
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

        tracing::debug!("networks collected {:?}", &networks);

        networks
    }
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

        let apps = forms::stack::Stack::try_from(&self.stack)?; 
        let  services = apps.custom.services()?;
        let  named_volumes = apps.custom.named_volumes()?;

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
