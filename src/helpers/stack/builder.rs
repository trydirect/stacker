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

// impl TryInto<Vec<Port>> for stack::Ports {
//     type Error = String;
//     fn try_into(self) -> Result<Vec<Port>, Self::Error> {
//         convert_shared_ports(self.shared_ports.clone())
//     }
// }


impl TryInto<Vec<Port>> for stack::Ports {
    type Error = String;
    fn try_into(self) -> Result<Vec<Port>, Self::Error> {
        convert_shared_ports(self.shared_ports.clone())
    }
}


fn convert_shared_ports(ports: Option<Vec<stack::Port>>) -> Result<Vec<Port>, String> {
    tracing::debug!("convert shared ports {:?}", &ports);
    let mut _ports: Vec<Port> = vec![];
    match ports {
        Some(ports) => {
            tracing::debug!("Ports >>>> {:?}", ports);
            for port in ports {
                let cp  = port.container_port
                    .unwrap_or("".to_string())
                    .parse::<u16>().map_err(|err| "Could not parse port".to_string() )?;
                let hp = port.host_port
                    .unwrap_or("".to_string())
                    .parse::<u16>().map_err(|err| "Could not parse port".to_string() )?;

                tracing::debug!("Port conversion result: cp: {:?} hp: {:?}", cp, hp);
                _ports.push(
                    Port {
                        target: cp,
                        host_ip: None,
                        published: Some(PublishedPort::Single(hp)),
                        protocol: None,
                        mode: None,
                    }
                );
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

impl DcBuilder {

    pub fn new(stack: Stack) -> Self {
        DcBuilder {
            config: Config::default(),
            stack: stack,
        }
    }


    pub fn build(&self) -> Option<String> {
        //
        // tracing::debug!("Start build docker compose from {:?}", &self.stack.body);
        // let _stack = serde_json::from_value::<StackForm>(self.stack.body.clone());
        // let mut services = indexmap::IndexMap::new();
        // match _stack  {
        //     Ok(apps) => {
        //         // tracing::debug!("stack item {:?}", apps.custom.web);
        //
        //         for app_type in apps.custom.web {
        //             let code = app_type.app.code.clone().to_owned();
        //             let mut service = Service {
        //                 image: Some(app_type.app.docker_image.to_string()),
        //                 ..Default::default()
        //             };
        //
        //             service.ports = Ports::Long(app_type.app.ports.unwrap().try_into().unwrap());
        //             service.restart = Some("always".to_owned());
        //             tracing::debug!("service 1 {:?}", &service);
        //             services.insert(
        //                 code,
        //                 Some(service),
        //             );
        //         }
        //
        //
        //         if let Some(srvs) = apps.custom.service {
        //
        //             if !srvs.is_empty() {
        //
        //                 for app_type in srvs {
        //                     let code = app_type.app.code.to_owned();
        //                     let mut service = Service {
        //                         image: Some(app_type.app.docker_image.to_string()),
        //                         ..Default::default()
        //                     };
        //
        //                     // if let Some(ports) = &app_type.app._ports {
        //                     //     tracing::debug!("service2 ports {:?}", ports);
        //                     //     service.ports = Ports::Long(ports.try_into().unwrap())
        //                     // }
        //
        //                     service.ports = Ports::Long(app_type.app.ports.unwrap().try_into().unwrap());
        //                     service.restart = Some("always".to_owned());
        //                     services.insert(
        //                         code,
        //                         Some(service),
        //                     );
        //                 }
        //                 // tracing::debug!("services {:?}", services);
        //             }
        //         }
        //
        //         if let Some(features) = apps.custom.feature {
        //
        //             if !features.is_empty() {
        //
        //                 for app_type in features {
        //                     let code = app_type.app.code.to_owned();
        //                     let mut service = Service {
        //                         // image: Some(app.dockerhub_image.as_ref().unwrap().to_owned()),
        //                         image: Some(app_type.app.docker_image.to_string()),
        //                         ..Default::default()
        //                     };
        //
        //                     service.ports = Ports::Long(app_type.app.ports.unwrap().try_into().unwrap());
        //                     // if let Some(ports) = &app_type.app._ports {
        //                     //     tracing::debug!("service3 ports {:?}", ports);
        //                     //     service.ports = Ports::Long(ports.try_into().unwrap())
        //                     // }
        //                     service.restart = Some("always".to_owned());
        //                     services.insert(
        //                         code,
        //                         Some(service),
        //                     );
        //                 }
        //                 // tracing::debug!("services### {:?}", &service);
        //             }
        //         }
        //         tracing::debug!("services {:?}", &services);
        //     }
        //     Err(e) => {
        //         tracing::debug!("Unpack stack form error {:?}", e);
        //         ()
        //     }
        // }
        //
        // let compose_content = Compose {
        //     version: Some("3.8".to_string()),
        //     services: {
        //         Services(services)
        //     },
        //     ..Default::default()
        // };
        //
        // let fname= format!("./files/{}.yml", self.stack.stack_id);
        // tracing::debug!("Saving docker compose to file {:?}", fname);
        // let target_file = std::path::Path::new(fname.as_str());
        // // serialize to string
        // let serialized = match serde_yaml::to_string(&compose_content) {
        //     Ok(s) => s,
        //     Err(e) => panic!("Failed to serialize docker-compose file: {}", e),
        // };
        // // serialize to file
        // std::fs::write(target_file, serialized.clone()).unwrap();
        //
        // Some(serialized)
        None
    }
}
