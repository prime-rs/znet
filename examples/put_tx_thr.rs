use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::info;
use znet::{
    config::{Args, Config},
    network::{Network, Subscriber},
    protocol::Message,
};

#[tokio::main(flavor = "multi_thread", worker_threads = 30)]
async fn main() -> Result<()> {
    common_x::log::init_log_filter("info");
    let args = Args::parse();
    let config: Config = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let mut stats = znet::stats::Stats::new(100000);
    let sub_callback = vec![Subscriber::new("topic", move |_msg| {
        stats.increment();
    })];

    let session = Network::serve(config.network_config, config.id, sub_callback, vec![]).await?;
    let sender = session.put_sender();

    let sender_handle = tokio::spawn(async move {
        // let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            // interval.tick().await;
            let msg = Message::new("topic/*", vec![0; 1024]);
            sender.send_async(msg).await.ok();
        }
    });
    waiting_for_shutdown().await;
    sender_handle.abort();
    info!("shutdown");
    Ok(())
}
