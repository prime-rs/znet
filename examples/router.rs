use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::info;
use znet::{
    config::{Args, Config},
    network::Network,
};

#[tokio::main(flavor = "multi_thread", worker_threads = 30)]
async fn main() -> Result<()> {
    common_x::log::init_log_filter("info");
    let args = Args::parse();
    let config: Config = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let _session = Network::serve(config.network_config, config.id, vec![], vec![]).await;
    waiting_for_shutdown().await;
    info!("shutdown");
    Ok(())
}
