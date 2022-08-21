use std::{sync::{Arc, Mutex, mpsc::channel, atomic::{AtomicBool, Ordering}, RwLock}, net::SocketAddr, time::Duration};
use clap::Parser;

use crossbeam::channel::TryRecvError;
use latency::Latency;
use log::{info, error};
use services::{AudioService, PeerMixer, OpusEncoder};
use client::Client;
use common::packets::{ClientMessage, self};
use env_logger::Env;

use crate::services::OpusDecoder;

mod services;
mod client;
mod latency;

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

  let addr = format!("{}:{}", args.address, args.port)
    .parse::<SocketAddr>().expect("Invalid server address.");

  let (mic_tx, mic_rx) = channel::<Vec<u8>>();
  let (peer_tx, peer_rx) = channel::<(u32, Vec<u8>)>();

  let mut client = Client::new("test".to_string(), mic_rx, peer_tx);
  client.connect(addr);

  let client_peer_connected = client.get_peer_connected_rx();

  let client_arc = Arc::new(client);
  let client_is_running = Arc::new(AtomicBool::new(true));
  
  let client_handle;
  {
    let client_is_running = client_is_running.clone();
    let client_arc = client_arc.clone();
    client_handle = std::thread::spawn(move|| {
      client_arc.service();
      client_is_running.store(false, Ordering::SeqCst);
    });
  }

  let mut mixer = Arc::new(RwLock::new(None));

  let audio_handle;
  {
    // let client = client_arc.clone();
    audio_handle = std::thread::spawn(move || {
      let mut audio = AudioService::builder()
        .with_latency(args.latency)
        .build().unwrap();

      {
        mixer.write().unwrap().replace(PeerMixer::new(
          audio.out_config().sample_rate.0,
          audio.out_latency(),
          audio.get_output_tx()
        ));
      }
      audio.start().unwrap();

      let input_consumer = audio.take_mic_rx().expect("microphone tx already taken");
      let mut encoder = OpusEncoder::new(audio.out_config().sample_rate.0).unwrap();
      encoder.add_output(mic_tx);

      while client_is_running.load(Ordering::SeqCst) {
        match peer_rx.try_recv() {
          Ok((id, packet)) => {
            let mixer = mixer.read().unwrap();
            let mixer = mixer.as_ref().unwrap();
            mixer.push(id, &packet);
          }
          Err(e) => {
            if e != std::sync::mpsc::TryRecvError::Empty {
              error!("Error receiving packet: {}", e);
            }
          }
        }
        if let Ok(sample) = input_consumer.try_recv() {
          encoder.push(sample);
        }
        {
          let mixer = mixer.read().unwrap();
          let mixer = mixer.as_ref().unwrap();
          match client_peer_connected.try_recv() {
            Ok(info) => {
              mixer.add_peer(info.id);
            },
            Err(e) if e != TryRecvError::Empty => {
              error!("Error receiving peer connections: {}", e);
            }
            _ => {}
        }
          mixer.tick();

        }
      }

      audio.stop();

    });
  }

  client_handle.join().unwrap();
  audio_handle.join().unwrap();
  
  
  Ok(())
}