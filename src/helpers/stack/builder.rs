use crate::helpers::stack::dctypes::{
    Compose,
    Port,
    Ports,
    PublishedPort,
    Service,
    Services
};
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

/// A builder for constructing docker compose.
#[derive(Clone, Debug)]
pub struct DcBuilder {
    config: Config,
    pub(crate) stack: Stack
}

impl TryInto<Vec<Port>> for stack::Ports {
    type Error = String;
    fn try_into(self) -> Result<Vec<Port>, Self::Error> {
        convert_shared_ports(self.shared_ports.clone().unwrap())
    }
}


fn convert_shared_ports(ports: Vec<String>) -> Result<Vec<Port>, String> {
    let mut _ports: Vec<Port> = vec![];
    for p in ports {
        let port = p.parse::<u16>().map_err(|e| e.to_string())?;
        _ports.push(Port {
            target: port,
            host_ip: None,
            published: Some(PublishedPort::Single(port)),
            protocol: None,
            mode: None,
        });
    }
    Ok(_ports)
}

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
                println!("stack item {:?}", apps.custom.web);

                for app_type in apps.custom.web {
                    let code = app_type.app.code.clone().to_owned();
                    let mut service = Service {
                        image: Some(app_type.app.docker_image.to_string()),
                        ..Default::default()
                    };

                    if let Some(ports) = &app_type.app.ports {
                        if !ports.shared_ports.clone()?.is_empty() {
                            service.ports = Ports::Long(app_type.app.ports?.try_into().unwrap())
                        }
                    }

                    service.restart = Some("always".to_owned());
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

                            if let Some(ports) = &app_type.app.ports {
                                if !ports.shared_ports.clone()?.is_empty() {
                                    service.ports = Ports::Long(app_type.app.ports?.try_into().unwrap())
                                }
                            }
                            service.restart = Some("always".to_owned());
                            services.insert(
                                code,
                                Some(service),
                            );
                        }
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

                            if let Some(ports) = &app_type.app.ports {
                                if !ports.shared_ports.clone()?.is_empty() {
                                    service.ports = Ports::Long(app_type.app.ports?.try_into().unwrap())
                                }
                            }
                            service.restart = Some("always".to_owned());
                            services.insert(
                                code,
                                Some(service),
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Unpack stack form {:?}", e);
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
        tracing::debug!("Save docker compose to file {:?}", fname);
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
