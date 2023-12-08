use std::collections::HashMap;
use indexmap::IndexMap;
use crate::helpers::stack::dctypes::{Compose, Port, Ports, PublishedPort, Service, Services, Volumes, Environment, Entrypoint, AdvancedVolumes, SingleValue};
use serde_yaml;
use crate::forms::{StackForm, stack};
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
    pub(crate) stack: Stack
}

impl TryInto<AdvancedVolumes> for &stack::Volume {
    type Error = String;
    fn try_into(self) -> Result<AdvancedVolumes, Self::Error> {

        let source = self.host_path.clone();
        let target = self.container_path.clone();
        tracing::debug!("Volume conversion result: source: {:?} target: {:?}", source, target);
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
        let cp  = self.container_port.clone()
            .unwrap_or("".to_string())
            .parse::<u16>().map_err(|err| "Could not parse port".to_string() )?;
        let hp = self.host_port.clone()
            .unwrap_or("".to_string())
            .parse::<u16>().map_err(|err| "Could not parse port".to_string() )?;

        tracing::debug!("Port conversion result: cp: {:?} hp: {:?}", cp, hp);
        Ok(Port {
            target: cp,
            host_ip: None,
            published: Some(PublishedPort::Single(hp)),
            protocol: None,
            mode: None,
        })
    }
}


fn convert_shared_ports(ports: Option<Vec<stack::Port>>) -> Result<Vec<Port>, String> {
    tracing::debug!("convert shared ports {:?}", &ports);
    let mut _ports: Vec<Port> = vec![];
    match ports {
        Some(ports) => {
            tracing::debug!("Ports >>>> {:?}", ports);
            for port in ports {
            }
        }
        None => {
            tracing::debug!("No ports defined by user");
            return Ok(_ports);
        }
    }

    tracing::debug!("ports {:?}", _ports);
    Ok(_ports)
}

// impl TryInto<IndexMap<String, Option<SingleValue>>> for HashMap<String, String> {
//     type Error = String;
//
//     fn try_into(self) -> Result<IndexMap<String, Option<SingleValue>>, Self::Error> {
//         let mut index_map = IndexMap::new();
//
//         for (key, value) in self {
//             let single_value = Some(SingleValue::String(value));
//             index_map.insert(key, single_value);
//         }
//
//         Ok(index_map)
//     }
// }

impl DcBuilder {

    pub fn new(stack: Stack) -> Self {
        DcBuilder {
            config: Config::default(),
            stack: stack,
        }
    }


    pub fn build(&self) -> Option<String> {

        tracing::debug!("Start build docker compose from {:?}", &self.stack.body);
        let _stack = serde_json::from_value::<StackForm>(self.stack.body.clone());
        let mut services = indexmap::IndexMap::new();
        match _stack  {
            Ok(apps) => {
                // tracing::debug!("stack item {:?}", apps.custom.web);

                for app_type in apps.custom.web {
                    let code = app_type.app.code.clone().to_owned();
                    let mut service = Service {
                        image: Some(app_type.app.docker_image.to_string()),
                        ..Default::default()
                    };

                    let ports: Vec<Port> = app_type.app.ports
                        .unwrap()
                        .iter()
                        .map(|x| x.try_into().unwrap())
                        .collect();

                    let volumes: Vec<AdvancedVolumes> = app_type.app.volumes
                        .unwrap()
                        .iter()
                        .map(|x| x.try_into().unwrap())
                        .collect();

                    let mut envs = IndexMap::new();
                    for item in app_type.app.environment.environment.unwrap() {
                        let items = item
                            .into_iter()
                            .map(|(k, v)| (k, Some(SingleValue::String(v.clone()))))
                            .collect::<IndexMap<_, _>>();

                        envs.extend(items);
                    }
                    // tracing::debug!("envs: {:?}", envs);
                    service.ports = Ports::Long(ports);
                    service.restart = Some("always".to_owned());
                    service.volumes = Volumes::Advanced(volumes);
                    service.environment = Environment::KvPair(envs);
                    // tracing::debug!("service 1 {:?}", &service);
                    services.insert(
                        code,
                        Some(service),
                    );
                }


                if let Some(srvs) = apps.custom.service {

                    if !srvs.is_empty() {

                        for app_type in srvs {
                            let code = app_type.app.code.to_owned();
                            let mut service = Service {
                                image: Some(app_type.app.docker_image.to_string()),
                                ..Default::default()
                            };

                            let ports: Vec<Port> = app_type.app.ports
                                .unwrap()
                                .iter()
                                .map(|x| x.try_into().unwrap())
                                .collect();

                            service.ports = Ports::Long(ports);
                            service.restart = Some("always".to_owned());
                            services.insert(
                                code,
                                Some(service),
                            );
                        }
                        // tracing::debug!("services {:?}", services);
                    }
                }

                if let Some(features) = apps.custom.feature {

                    if !features.is_empty() {

                        for app_type in features {
                            let code = app_type.app.code.to_owned();
                            let mut service = Service {
                                // image: Some(app.dockerhub_image.as_ref().unwrap().to_owned()),
                                image: Some(app_type.app.docker_image.to_string()),
                                ..Default::default()
                            };

                            let ports: Vec<Port> = app_type.app.ports
                                .unwrap()
                                .iter()
                                .map(|x| x.try_into().unwrap())
                                .collect();
                            service.ports = Ports::Long(ports);
                            service.restart = Some("always".to_owned());
                            services.insert(
                                code,
                                Some(service),
                            );
                        }
                        // tracing::debug!("services### {:?}", &service);
                    }
                }
                tracing::debug!("services {:?}", &services);
            }
            Err(e) => {
                tracing::debug!("Unpack stack form error {:?}", e);
                ()
            }
        }


        let compose_content = Compose {
            version: Some("3.8".to_string()),
            services: {
                Services(services)
            },
            ..Default::default()
        };

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
