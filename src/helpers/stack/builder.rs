use indexmap::IndexMap;
use crate::helpers::stack::dctypes::{Compose, Port, Ports, PublishedPort, Service, Services,
                                     Volumes, Environment, Entrypoint, AdvancedVolumes, SingleValue,
                                     Networks, TopLevelVolumes, ComposeVolume, ComposeNetwork,
                                     ComposeNetworks, MapOrEmpty, ComposeNetworkSettingDetails,
                                     NetworkSettings};
use serde_yaml;
use crate::forms::{StackForm, stack, App, Volume, Network, NetworkDriver};
use crate::models::stack::Stack;
#[derive(Clone, Debug)]
struct Config {}

impl Default for Config {
    fn default() -> Self {
        Config {}
    }
}

impl Default for Port{
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
    pub(crate) stack: Stack,
}

impl TryInto<AdvancedVolumes> for Volume {
    // service volumes
    type Error = String;
    fn try_into(self) -> Result<AdvancedVolumes, Self::Error> {

        let source = self.host_path.clone();
        let target = self.container_path.clone();
        tracing::debug!("Volume conversion result: source: {:?} target: {:?}", source, target);
        let _type = if is_named_docker_volume(source.as_ref().unwrap()) {
            "volume"
        } else {
            "bind"
        };

        Ok(AdvancedVolumes {
            source,
            target: target.unwrap_or("".to_string()),
            _type: _type.to_string(),
            read_only: false,
            bind: None,
            volume: None,
            tmpfs: None,
        })
    }
}

impl TryInto<Port> for stack::Port {
    type Error = String;
    fn try_into(self) -> Result<Port, Self::Error> {
        let cp = self.container_port
            .clone()
            .parse::<u16>()
            .map_err(|err| "Could not parse container port".to_string() )?;

        let hp = match self.host_port.clone() {
            Some(hp) => {
                if hp.is_empty() {
                    None
                } else {
                    match hp.parse::<u16>() {
                        Ok(port) => Some(PublishedPort::Single(port)),
                        Err(_) => {
                            tracing::debug!("Could not parse host port: {}", hp);
                            None
                        }
                    }
                }
            }
            _ => None
        };

        tracing::debug!("Port conversion result: cp: {:?} hp: {:?}", cp, hp);

        Ok(Port {
            target: cp,
            host_ip: None,
            published: hp,
            protocol: None,
            mode: None,
        })
    }
}

impl TryInto<Networks> for stack::ServiceNetworks {
    type Error = ();
    fn try_into(self) -> Result<Networks, Self::Error> {
        let mut default_networks = vec![];
        let nets = match self.network {
            Some(mut _nets) => {
                    _nets.append(&mut default_networks);
                _nets
            }
            None => {
               default_networks
            }
        };
        Ok(Networks::Simple(nets))
    }
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

trait TryIntoService {
    fn try_into_service(&self) -> Service;
}

impl TryIntoService for App {
    fn try_into_service(&self) -> Service {
        let mut service = Service {
            image: Some(self.docker_image.to_string()),
            ..Default::default()
        };

        let networks: Networks = self.network
            .clone()
            .try_into()
            .unwrap_or_default();

        let ports: Vec<Port> = self.shared_ports
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|x| x.try_into().unwrap())
            .collect();

        let volumes: Vec<AdvancedVolumes> = self.volumes
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|x| {
                x.try_into().unwrap()
            })
            .collect();

        let mut envs = IndexMap::new();
        let items = self.environment.environment.clone().unwrap_or_default()
            .into_iter()
            .map(|env_var| (env_var.key, Some(SingleValue::String(env_var.value.clone()))))
            .collect::<IndexMap<_,_>>();

        envs.extend(items);

        service.networks = networks;
        service.ports = Ports::Long(ports);
        service.restart = Some(self.restart.clone());
        service.volumes = Volumes::Advanced(volumes);
        service.environment = Environment::KvPair(envs);

        service
    }
}

impl Into<IndexMap<String, Option<SingleValue>>> for NetworkDriver {
    fn into(self) -> IndexMap<String, Option<SingleValue>> {
        IndexMap::new()
    }
}

impl Into<NetworkSettings> for Network {

    fn into(self) -> NetworkSettings {

        NetworkSettings {
            attachable: self.attachable.unwrap_or(false),
            driver: self.driver.clone(),
            driver_opts: self.driver_opts.unwrap_or_default().into(), // @todo
            enable_ipv6: self.enable_ipv6.unwrap_or(false),
            internal: self.internal.unwrap_or(false),
            external: Some(ComposeNetwork::Bool(self.external.unwrap_or(false))),
            ipam: None,                                                       // @todo
            labels: Default::default(),
            name: Some(self.name.clone()),
        }
    }
}
impl Into<IndexMap<String, MapOrEmpty<NetworkSettings>>> for stack::ComposeNetworks {
    fn into(self) -> IndexMap<String, MapOrEmpty<NetworkSettings>> {

        // let mut default_networks = vec![Network::default()];
        let mut default_networks = vec![];

        let networks = match self.networks {
            None => {
                default_networks
            }
            Some(mut nets) => {
                if !nets.is_empty() {
                    nets.append(&mut default_networks);
                }
                nets
            }
        };

        let networks = networks
            .into_iter()
            .map(|net| {
                (net.name.clone(), MapOrEmpty::Map(net.into()))
            }
            )
            .collect::<IndexMap<String, _>>();

        tracing::debug!("networks collected {:?}", &networks);

        networks
    }
}

impl Into<ComposeVolume> for Volume {
    fn into(self) -> ComposeVolume {
        // let's create a symlink to /var/docker/volumes in project docroot
        let mut driver_opts = IndexMap::default();
        let host_path = self.host_path.unwrap();
        driver_opts.insert(String::from("type"), Some(SingleValue::String("none".to_string())));
        driver_opts.insert(String::from("o"), Some(SingleValue::String("bind".to_string())));
        // @todo move to config stack docroot on host
        let path = format!("/root/stack/{}", &host_path);
        driver_opts.insert(String::from("device"), Some(SingleValue::String(path)));

        ComposeVolume {
            driver: Some(String::from("local")),
            driver_opts: driver_opts,
            external: None,
            labels: Default::default(),
            name: Some(host_path)
        }
    }
}

#[tracing::instrument(name = "extract_named_volumes")]
pub fn extract_named_volumes(app: App) -> IndexMap<String, MapOrEmpty<ComposeVolume>> {

    let mut named_volumes = IndexMap::default();
    if app.volumes.is_none() {
        return named_volumes;
    }

    let volumes = app.volumes
        .unwrap()
        .into_iter()
        .filter(|volume| is_named_docker_volume(
            volume.host_path.clone().unwrap().as_str())
        )
        .map(|volume| {
            let k = volume.host_path.clone().unwrap();
            (k.clone(), MapOrEmpty::Map(volume.into()))
        })
        .collect::<IndexMap<String, MapOrEmpty<ComposeVolume>>>();

    named_volumes.extend(volumes);
    tracing::debug!("Named volumes: {:?}", named_volumes);

    named_volumes
}

impl DcBuilder {

    pub fn new(stack: Stack) -> Self {
        DcBuilder {
            config: Config::default(),
            stack,
        }
    }

    pub fn build(&self) -> Option<String> {
        tracing::debug!("Start build docker compose from {:?}", &self.stack.body);
        let mut compose_content = Compose {
            version: Some("3.8".to_string()),
            ..Default::default()
        };
        let _stack = serde_json::from_value::<StackForm>(self.stack.body.clone());
        let mut services = IndexMap::new();
        let mut named_volumes = IndexMap::default();

        match _stack {
            Ok(apps) => {
                for app_type in &apps.custom.web {
                    let service = app_type.app.try_into_service();
                    services.insert(app_type.app.code.clone().to_owned(), Some(service));
                    named_volumes.extend(extract_named_volumes(app_type.app.clone()));
                }

                if let Some(srvs) = apps.custom.service {
                    for app_type in srvs {
                        let service = app_type.app.try_into_service();
                        services.insert(app_type.app.code.clone().to_owned(), Some(service));
                        named_volumes.extend(extract_named_volumes(app_type.app.clone()));
                    }
                }

                if let Some(features) = apps.custom.feature {
                    for app_type in features {
                        let service = app_type.app.try_into_service();
                        services.insert(app_type.app.code.clone().to_owned(), Some(service));
                        named_volumes.extend(extract_named_volumes(app_type.app.clone()));
                    }
                }

                let networks = apps.custom.networks.clone();
                compose_content.networks = ComposeNetworks(networks.into());

                if !named_volumes.is_empty() {
                    compose_content.volumes = TopLevelVolumes(named_volumes);
                }

            }
            Err(e) => {
                tracing::debug!("Unpack stack form error {:?}", e);
            }
        }
        tracing::debug!("services {:?}", &services);
        compose_content.services = Services(services);


        let fname= format!("./files/{}.yml", self.stack.stack_id);
        tracing::debug!("Saving docker compose to file {:?}", fname);
        let target_file = std::path::Path::new(fname.as_str());
        // serialize to string
        let serialized = match serde_yaml::to_string(&compose_content) {
            Ok(s) => s,
            Err(e) => panic!("Failed to serialize docker-compose file: {}", e),
        };
        // serialize to file
        std::fs::write(target_file, serialized.clone()).unwrap();

        Some(serialized)
    }
}
