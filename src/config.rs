use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::network::NetworkConfig;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "config/config.toml")]
    pub config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub id: [u8; 16],

    pub network_config: NetworkConfig,
}
