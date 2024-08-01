use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::{debug, info};
use znet::{
    config::{Args, Config},
    network::{Network, Queryable},
    protocol::Message,
};

#[tokio::main(flavor = "multi_thread", worker_threads = 30)]
async fn main() -> Result<()> {
    common_x::log::init_log_filter("info");
    let args = Args::parse();
    let config: Config = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let mut stats = znet::stats::Stats::new(10000);
    let queryable_callback = vec![Queryable::new("topic", move |_net_msg| -> Message {
        stats.increment();
        debug!("reply...");
        Message::new("topic", vec![0; 1024])
    })];

    let _session =
        Network::serve(config.network_config, config.id, vec![], queryable_callback).await;
    waiting_for_shutdown().await;
    info!("shutdown");
    Ok(())
}
