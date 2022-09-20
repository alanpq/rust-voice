use std::{net::SocketAddr, sync::{atomic::{AtomicBool, Ordering}, Arc}};

use app::App;
use clap::Parser;
use env_logger::Env;

use log::{info, error};

use ctrlc;

mod voice;
mod mic;
mod util;
mod latency;
mod client;
mod cpal;
mod decoder;
mod app;

#[derive(Parser, Debug)]
#[clap(name="Rust Voice Server")]
struct Args {
  #[clap(value_parser)]
  address: String,
  #[clap(value_parser = clap::value_parser!(u16).range(1..), short='p', long="port", default_value_t=8080)]
  port: u16,
  #[clap(value_parser, long="latency", default_value_t=150.)]
  latency: f32,
}

fn main() -> Result<(), anyhow::Error> {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
  let args = Args::parse();

  let running = Arc::new(AtomicBool::new(true));

  {
    let running = running.clone();
    ctrlc::set_handler(move || {
      running.store(false, Ordering::SeqCst);
    })?;
  }

  let mut app = App::new("test".to_string(), args.latency)?;
  
  let addr: SocketAddr = format!("{}:{}", args.address, args.port).parse()?;
  app.start(addr)?;
  while running.load(Ordering::Relaxed) {
    app.poll()?;
  }
  app.stop();
  
  Ok(())
}