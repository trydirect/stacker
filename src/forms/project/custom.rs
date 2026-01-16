use crate::forms;
use crate::forms::project::Network;
use docker_compose_types as dctypes;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Custom {
    #[validate]
    pub web: Vec<forms::project::Web>,
    #[validate]
    pub feature: Option<Vec<forms::project::Feature>>,
    #[validate]
    pub service: Option<Vec<forms::project::Service>>,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub custom_stack_code: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub project_git_url: Option<String>,
    pub custom_stack_category: Option<Vec<String>>,
    pub custom_stack_short_description: Option<String>,
    pub custom_stack_description: Option<String>,
    // #[validate(min_length = 3)]
    // #[validate(max_length = 255)]
    pub project_name: Option<String>,
    pub project_overview: Option<String>,
    pub project_description: Option<String>,
    #[serde(flatten)]
    pub networks: forms::project::ComposeNetworks, // all networks
}

fn matches_network_by_id(id: &String, networks: &Vec<Network>) -> Option<String> {
    for n in networks.into_iter() {
        if id == &n.id {
            tracing::debug!("matches:  {:?}", n.name);
            return Some(n.name.clone());
        }
    }
    None
}

pub fn replace_id_with_name(
    service_networks: dctypes::Networks,
    all_networks: &Vec<Network>,
) -> Vec<String> {
    match service_networks {
        dctypes::Networks::Simple(nets) => nets
            .iter()
            .map(|id| {
                if let Some(name) = matches_network_by_id(&id, all_networks) {
                    name
                } else {
                    "".to_string()
                }
            })
            .collect::<Vec<String>>(),
        _ => vec![],
    }
}

impl Custom {
    pub fn services(&self) -> Result<IndexMap<String, Option<dctypes::Service>>, String> {
        let mut services = IndexMap::new();

        let all_networks = self.networks.networks.clone().unwrap_or(vec![]);

        for app_type in &self.web {
            let service = app_type.app.try_into_service(&all_networks)?;
            services.insert(app_type.app.code.clone().to_owned(), Some(service));
        }

        if let Some(srvs) = &self.service {
            for app_type in srvs {
                let service = app_type.app.try_into_service(&all_networks)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
            }
        }

        if let Some(features) = &self.feature {
            for app_type in features {
                let service = app_type.app.try_into_service(&all_networks)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
            }
        }

        Ok(services)
    }

    pub fn named_volumes(
        &self,
    ) -> Result<IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>>, String> {
        let mut named_volumes = IndexMap::new();

        for app_type in &self.web {
            named_volumes.extend(app_type.app.named_volumes());
        }

        if let Some(srvs) = &self.service {
            for app_type in srvs {
                named_volumes.extend(app_type.app.named_volumes());
            }
        }

        if let Some(features) = &self.feature {
            for app_type in features {
                named_volumes.extend(app_type.app.named_volumes());
            }
        }

        Ok(named_volumes)
    }
}
