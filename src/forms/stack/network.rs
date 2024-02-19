use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use crate::forms::stack;
use docker_compose_types as dctypes;
use indexmap::IndexMap;
use crate::forms::stack::NetworkDriver;


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
pub struct Network {
    pub(crate) id: String,
    pub(crate) attachable: Option<bool>,
    pub(crate) driver: Option<String>,
    pub(crate) driver_opts: Option<NetworkDriver>,
    pub(crate) enable_ipv6: Option<bool>,
    pub(crate) internal: Option<bool>,
    pub(crate) external: Option<bool>,
    pub(crate) ipam: Option<String>,
    pub(crate) labels: Option<String>,
    pub(crate) name: String,
}


impl Default for Network {
    fn default() -> Self {
        // The case when we need at least one external network to be preconfigured
        Network {
            id: "default_network".to_string(),
            attachable: None,
            driver: None,
            driver_opts: Default::default(),
            enable_ipv6: None,
            internal: None,
            external: Some(true),
            ipam: None,
            labels: None,
            name: "default_network".to_string(),
        }
    }
}

impl Into<dctypes::NetworkSettings> for Network {

    fn into(self) -> dctypes::NetworkSettings {

        // default_network is always external=true
        let is_default = self.name == String::from("default_network");
        let external = is_default || self.external.unwrap_or(false);

        dctypes::NetworkSettings {
            attachable: self.attachable.unwrap_or(false),
            driver: self.driver.clone(),
            driver_opts: self.driver_opts.unwrap_or_default().into(), // @todo
            enable_ipv6: self.enable_ipv6.unwrap_or(false),
            internal: self.internal.unwrap_or(false),
            external: Some(dctypes::ComposeNetwork::Bool(external)),
            ipam: None,                                               // @todo
            labels: Default::default(),
            name: Some(self.name.clone()),
        }
    }
}
impl Into<IndexMap<String, dctypes::MapOrEmpty<dctypes::NetworkSettings>>> for stack::ComposeNetworks {
    fn into(self) -> IndexMap<String, dctypes::MapOrEmpty<dctypes::NetworkSettings>> {

        // let mut default_networks = vec![Network::default()];
        let mut default_networks = vec![];

        let networks = match self.networks {
            None => {
                default_networks
            }
            Some(mut nets) => {
                if !nets.is_empty() {
                    nets.append(&mut default_networks);
                }
                nets
            }
        };

        let networks = networks
            .into_iter()
            .map(|net| {
                (net.name.clone(), dctypes::MapOrEmpty::Map(net.into()))
            }
            )
            .collect::<IndexMap<String, _>>();

        tracing::debug!("networks collected {:?}", &networks);

        networks
    }
}
