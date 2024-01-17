use serde::{Deserialize, Serialize};
use docker_compose_types as dctypes;
use indexmap::IndexMap;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposeNetworks {
    pub networks: Option<Vec<String>>,
}

impl Into<IndexMap<String, dctypes::MapOrEmpty<dctypes::NetworkSettings>>> for ComposeNetworks {
    fn into(self) -> IndexMap<String, dctypes::MapOrEmpty<dctypes::NetworkSettings>> {
        let mut networks = vec!["default_network".to_string()];
        if self.networks.is_some() {
            networks.append(&mut self.networks.unwrap());
        }
        let networks = networks
            .into_iter()
            .map(|net| {
                (
                    net,
                    dctypes::MapOrEmpty::Map(dctypes::NetworkSettings {
                        attachable: false,
                        driver: None,
                        driver_opts: Default::default(),
                        enable_ipv6: false,
                        internal: false,
                        // external: None,
                        external: Some(dctypes::ComposeNetwork::Bool(true)),
                        ipam: None,
                        labels: Default::default(),
                        name: Some("default".to_string()),
                    }),
                )
            })
            .collect::<IndexMap<String, _>>();

        tracing::debug!("networks collected {:?}", &networks);

        networks
    }
}
