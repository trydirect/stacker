use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceNetworks {
    pub network: Option<Vec<String>>,
}

impl TryFrom<&ServiceNetworks> for dctypes::Networks {
    type Error = ();

    fn try_from(service_networks: &ServiceNetworks) -> Result<dctypes::Networks, Self::Error> {
        let nets = match service_networks.network.as_ref() {
            Some(_nets) => {
                _nets.clone()
           }
            None => {
               vec![]
            }
        };
        Ok(dctypes::Networks::Simple(nets.into()))
    }
}
