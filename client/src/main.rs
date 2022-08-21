use std::{sync::{Arc, Mutex, mpsc::channel, atomic::{AtomicBool, Ordering}}, net::SocketAddr};
use clap::Parser;

use audio::AudioService;
use client::Client;
use common::packets::ClientMessage;

mod audio;
mod client;

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
  env_logger::builder().filter_level(log::LevelFilter::Info).init();

  let args = Args::parse();

  let addr = format!("{}:{}", args.address, args.port)
    .parse::<SocketAddr>().expect("Invalid server address.");

  let (mic_tx, mic_rx) = channel::<Vec<u8>>();
  let (peer_tx, peer_rx) = channel::<(u32, Vec<u8>)>();

  let mut client = Client::new("test".to_string(), mic_rx, peer_tx);
  client.connect(addr);

  let client_arc = Arc::new(Mutex::new(client));

  let handle;
  {
    let client_arc = client_arc.clone();
    handle = std::thread::spawn(move|| {
      client_arc.lock().unwrap().service();
    });
  }

  let audio_running_lock = Arc::new(Mutex::new(()));
  let audio_guard = audio_running_lock.lock().unwrap();

  {
    let audio_running_lock = audio_running_lock.clone();
    std::thread::spawn(move || {
      let mut audio = AudioService::builder()
        .with_channels(mic_tx, peer_rx)
        .with_latency(args.latency)
        .build().unwrap();
      audio.start().unwrap();

      let _ = audio_running_lock.lock().unwrap(); // this will hang until the lock is released
      
      audio.stop();

    });
  }

  handle.join().unwrap();
  drop(audio_guard); // release the mutex, allowing the audio thread to finish
  
  
  Ok(())
}