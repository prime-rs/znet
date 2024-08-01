use serde::{Deserialize, Serialize};

use crate::network::NetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub id: [u8; 16],

    pub network_config: NetworkConfig,
}
