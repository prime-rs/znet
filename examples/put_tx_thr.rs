use std::time::Instant;

use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::info;
use znet::{
    config::Config,
    network::{Network, Subscriber},
    protocol::Message,
};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value = "config/config.toml")]
    pub config: String,
}

#[derive(Debug)]
pub struct Stats {
    round_count: usize,
    round_size: usize,
    finished_rounds: usize,
    round_start: Instant,
    global_start: Option<Instant>,
}

impl Stats {
    pub fn new(round_size: usize) -> Self {
        Stats {
            round_count: 0,
            round_size,
            finished_rounds: 0,
            round_start: Instant::now(),
            global_start: None,
        }
    }
    pub fn increment(&mut self) {
        if self.round_count == 0 {
            self.round_start = Instant::now();
            if self.global_start.is_none() {
                self.global_start = Some(self.round_start)
            }
            self.round_count += 1;
        } else if self.round_count < self.round_size {
            self.round_count += 1;
        } else {
            self.print_round();
            self.finished_rounds += 1;
            self.round_count = 0;
        }
    }
    fn print_round(&self) {
        let elapsed = self.round_start.elapsed().as_secs_f64();
        let throughtput = (self.round_size as f64) / elapsed;
        info!("{throughtput} msg/s");
    }
}
impl Drop for Stats {
    fn drop(&mut self) {
        let elapsed = self
            .global_start
            .unwrap_or(Instant::now())
            .elapsed()
            .as_secs_f64();
        let total = self.round_size * self.finished_rounds + self.round_count;
        let throughtput = total as f64 / elapsed;
        info!("Received {total} messages over {elapsed:.2}s: {throughtput}msg/s");
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 30)]
async fn main() -> Result<()> {
    common_x::log::init_log_filter("info");
    let args = Args::parse();
    let config: Config = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let mut stats = Stats::new(100000);
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
