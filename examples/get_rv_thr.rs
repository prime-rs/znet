use std::time::Instant;

use clap::Parser;
use color_eyre::Result;
use common_x::signal::waiting_for_shutdown;
use tracing::{debug, info};
use znet::{
    protocol::Message,
    znet::{Queryable, Znet, ZnetConfig},
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
    let config: ZnetConfig = common_x::configure::file_config(&args.config)?;

    info!("config: {:#?}", config);

    let mut stats = Stats::new(10000);
    let queryable_callback = vec![Queryable::new("topic", move |_net_msg| -> Message {
        stats.increment();
        debug!("reply...");
        Message::new("topic", vec![0; 1024])
    })];

    let _session = Znet::serve(config, vec![], queryable_callback).await;
    waiting_for_shutdown().await;
    info!("shutdown");
    Ok(())
}
