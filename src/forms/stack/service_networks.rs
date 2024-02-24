use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceNetworks {
    pub network: Option<Vec<String>>,
}

impl TryFrom<&ServiceNetworks> for dctypes::Networks {
    type Error = ();

    fn try_from(service_networks: &ServiceNetworks) -> Result<dctypes::Networks, Self::Error> {
        let mut default_networks = vec![];
        let nets = match service_networks.network.as_ref() {
            Some(mut _nets) => {
                    _nets.append(&mut default_networks);
                _nets.clone()
           }
            None => {
               default_networks
            }
        };
        Ok(dctypes::Networks::Simple(nets.into()))
    }
}
