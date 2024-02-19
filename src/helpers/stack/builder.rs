use crate::forms;
use docker_compose_types as dctypes;
use crate::models;
use serde_yaml;
use crate::helpers::stack::*;
use tracing::Value;


/// A builder for constructing docker compose.
#[derive(Clone, Debug)]
pub struct DcBuilder {
    config: Config,
    pub(crate) stack: models::Stack,
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
        let mut compose_content = dctypes::Compose {
            version: Some("3.8".to_string()),
            ..Default::default()
        };

// <<<<<<< HEAD
        let apps = forms::stack::Stack::try_from(&self.stack)?; 
        let  services = apps.custom.services()?;
        let  named_volumes = apps.custom.named_volumes()?;

        // let all_networks = &apps.custom.networks.networks.clone().unwrap_or(vec![]);
        let networks = apps.custom.networks.clone();
        compose_content.networks = dctypes::ComposeNetworks(networks.into());

        if !named_volumes.is_empty() {
            compose_content.volumes = dctypes::TopLevelVolumes(named_volumes);
// =======
//         match _stack {
//             Ok(apps) => {
//
//                 for app_type in &apps.custom.web {
//                     let mut service = app_type.app.try_into_service();
//                     let service_networks = service.networks.clone();
//                     let networks = replace_id_with_name(service_networks, all_networks);
//                     service.networks = Networks::Simple(networks);
//                     services.insert(app_type.app.code.clone().to_owned(), Some(service));
//                     named_volumes.extend(extract_named_volumes(app_type.app.clone()));
//                 }
//
//                 if let Some(srvs) = apps.custom.service {
//                     for app_type in srvs {
//                         let mut service = app_type.app.try_into_service();
//                         let service_networks = service.networks.clone();
//                         let networks = replace_id_with_name(service_networks, all_networks);
//                         service.networks = Networks::Simple(networks);
//                         services.insert(app_type.app.code.clone().to_owned(), Some(service));
//                         named_volumes.extend(extract_named_volumes(app_type.app.clone()));
//                     }
//                 }
//
//                 if let Some(features) = apps.custom.feature {
//                     for app_type in features {
//                         let mut service = app_type.app.try_into_service();
//                         let service_networks = service.networks.clone();
//                         let networks = replace_id_with_name(service_networks, all_networks);
//                         service.networks = Networks::Simple(networks);
//                         services.insert(app_type.app.code.clone().to_owned(), Some(service));
//                         named_volumes.extend(extract_named_volumes(app_type.app.clone()));
//                     }
//                 }
//
//                 let networks = apps.custom.networks.clone();
//                 compose_content.networks = ComposeNetworks(networks.into());
//
//                 if !named_volumes.is_empty() {
//                     compose_content.volumes = TopLevelVolumes(named_volumes);
//                 }
//
//             }
//             Err(e) => {
//                 tracing::debug!("Unpack stack form error {:?}", e);
//             }
// >>>>>>> issue-16
        }

        tracing::debug!("services {:?}", &services);
        compose_content.services = dctypes::Services(services);

        let fname = format!("./files/{}.yml", self.stack.stack_id);
        tracing::debug!("Saving docker compose to file {:?}", fname);
        let target_file = std::path::Path::new(fname.as_str());
        let serialized = serde_yaml::to_string(&compose_content)
            .map_err(|err| format!("Failed to serialize docker-compose file: {}", err))?;

        std::fs::write(target_file, serialized.clone()).map_err(|err| format!("{}", err))?;

        Ok(serialized)
    }
}
