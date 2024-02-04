use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use crate::forms;
use indexmap::IndexMap;
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Custom {
    pub web: Vec<forms::stack::Web>,
    pub feature: Option<Vec<forms::stack::Feature>>,
    pub service: Option<Vec<forms::stack::Service>>,
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub servers_count: u32,
    #[validate(min_length = 3)]
    #[validate(max_length = 50)]
    pub custom_stack_code: String,
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub project_git_url: Option<String>,
    pub custom_stack_category: Option<Vec<String>>,
    pub custom_stack_short_description: Option<String>,
    pub custom_stack_description: Option<String>,
    #[validate(min_length = 3)]
    #[validate(max_length = 255)]
    pub project_name: String,
    pub project_overview: Option<String>,
    pub project_description: Option<String>,
    #[serde(flatten)]
    pub networks: forms::stack::ComposeNetworks, // all networks
}

impl Custom {
    pub fn services(&self) -> Result<IndexMap<String, Option<dctypes::Service>>, String> {
        let mut services = IndexMap::new();

        for app_type in &self.web {
            let service = dctypes::Service::try_from(&app_type.app)?;
            services.insert(app_type.app.code.clone().to_owned(), Some(service));
        }

        if let Some(srvs) = &self.service {
            for app_type in srvs {
                let service = dctypes::Service::try_from(&app_type.app)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
            }
        }

        if let Some(features) = &self.feature {
            for app_type in features {
                let service = dctypes::Service::try_from(&app_type.app)?;
                services.insert(app_type.app.code.clone().to_owned(), Some(service));
            }
        }

        Ok(services)
    }

    pub fn named_volumes(&self) -> Result<IndexMap<String, dctypes::MapOrEmpty<dctypes::ComposeVolume>>, String> {
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
