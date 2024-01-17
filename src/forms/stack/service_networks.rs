use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceNetworks {
    pub network: Option<Vec<String>>,
}

impl TryFrom<&ServiceNetworks> for dctypes::Networks {
    type Error = ();

    fn try_from(service_networks: &ServiceNetworks) -> Result<dctypes::Networks, Self::Error> {
        let mut result = vec!["default_network".to_string()];
        service_networks.network.as_ref().map(|networks| {
            for n in networks {
                result.push(n.to_string());
            }
        });

        Ok(dctypes::Networks::Simple(result))
    }
}
