use crate::forms;
use docker_compose_types as dctypes;
use crate::models;
use serde_yaml;
use crate::helpers::project::*;
use tracing::Value;


/// A builder for constructing docker compose.
#[derive(Clone, Debug)]
pub struct DcBuilder {
    config: Config,
    pub(crate) project: models::Project,
}


impl DcBuilder {
    pub fn new(project: models::Project) -> Self {
        DcBuilder {
            config: Config::default(),
            project,
        }
    }

    #[tracing::instrument(name = "building project")]
    pub fn build(&self) -> Result<String, String> {
        let mut compose_content = dctypes::Compose {
            version: Some("3.8".to_string()),
            ..Default::default()
        };

        let apps = forms::project::ProjectForm::try_from(&self.project)?;
        tracing::debug!("apps {:?}", &apps);
        let services = apps.custom.services()?;
        tracing::debug!("services {:?}", &services);
        let named_volumes = apps.custom.named_volumes()?;

        tracing::debug!("named volumes {:?}", &named_volumes);
        // let all_networks = &apps.custom.networks.networks.clone().unwrap_or(vec![]);
        let networks = apps.custom.networks.clone();
        compose_content.networks = dctypes::ComposeNetworks(networks.into());

        if !named_volumes.is_empty() {
            compose_content.volumes = dctypes::TopLevelVolumes(named_volumes);
        }

        compose_content.services = dctypes::Services(services);

        let fname = format!("./files/{}.yml", self.project.stack_id);
        tracing::debug!("Saving docker compose to file {:?}", fname);
        let target_file = std::path::Path::new(fname.as_str());
        let serialized = serde_yaml::to_string(&compose_content)
            .map_err(|err| format!("Failed to serialize docker-compose file: {}", err))?;

        std::fs::write(target_file, serialized.clone()).map_err(|err| format!("{}", err))?;

        Ok(serialized)
    }
}
