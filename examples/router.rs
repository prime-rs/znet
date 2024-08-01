use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::info;
use znet::znet::{Znet, ZnetConfig};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "config/config.toml")]
    pub config: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 30)]
async fn main() -> Result<()> {
    common_x::log::init_log_filter("info");
    let args = Args::parse();
    let config: ZnetConfig = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let _session = Znet::serve(config, vec![], vec![]).await;
    waiting_for_shutdown().await;
    info!("shutdown");
    Ok(())
}
