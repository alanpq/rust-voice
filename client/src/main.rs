use std::net::SocketAddr;

use app::App;
use clap::Parser;
use env_logger::Env;

use log::{info, error};

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

use kira::{manager::{
  AudioManager, AudioManagerSettings,
}, sound::Sound, dsp::Frame, Volume};
use uuid::Uuid;
use voice::{VoiceSoundData, VoiceSoundSettings};

use crate::{client::Client, mic::MicService, voice::VoiceSoundHandle, cpal::CpalBackend, decoder::OpusDecoder};

fn main() -> Result<(), anyhow::Error> {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
  let args = Args::parse();

  let mut app = App::new("test".to_string(), args.latency)?;
  
  let addr: SocketAddr = format!("{}:{}", args.address, args.port).parse()?;
  app.start(addr)?;
  loop {
    app.poll()?;
  }
  
  Ok(())
}