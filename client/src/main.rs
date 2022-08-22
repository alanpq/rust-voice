use std::{sync::{Arc, Mutex, mpsc::{channel, Sender, Receiver}, atomic::{AtomicBool, Ordering}, RwLock}, net::SocketAddr, time::Duration, thread::JoinHandle};
use clap::Parser;

use crossbeam::channel::{TryRecvError, self};
use latency::Latency;
use log::{info, error};
use services::{AudioService, PeerMixer, OpusEncoder};
use client::Client;
use common::{packets::{ClientMessage, self}, UserInfo};
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

struct SharedState {
  pub args: Args,
  pub client_running: AtomicBool,
  pub peer_connect_rx: channel::Receiver<UserInfo>,
}

fn audio_thread(state: Arc<SharedState>, peer_rx: Receiver<(u32, Vec<u8>)>,mic_tx: Sender<Vec<u8>>) -> JoinHandle<()> {
  std::thread::spawn(move || {
    let mut audio = AudioService::builder()
      .with_latency(state.args.latency)
      .build().unwrap();

    let mixer = PeerMixer::new(
      audio.out_config().sample_rate.0,
      audio.out_latency(),
      audio.get_output_tx()
    );
    audio.start().unwrap();

    let input_consumer = audio.take_mic_rx().expect("microphone tx already taken");
    let mut encoder = OpusEncoder::new(audio.out_config().sample_rate.0).unwrap();
    encoder.add_output(mic_tx);

    while state.client_running.load(Ordering::SeqCst) {
      match peer_rx.try_recv() {
        Ok((id, packet)) => {
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
        match state.peer_connect_rx.try_recv() {
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

  })
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
  let peer_connect_rx = client.get_peer_connected_rx();


  let state = Arc::new(SharedState {
    args,
    client_running: AtomicBool::new(true),
    peer_connect_rx,
  });
  
  let client = Arc::new(client);
  let client_handle;
  {
    let state = state.clone();
    client_handle = std::thread::spawn(move|| {
      client.service();
      state.client_running.store(false, Ordering::SeqCst);
    });
  }

  let audio_handle = audio_thread(state, peer_rx, mic_tx);

  client_handle.join().unwrap();
  audio_handle.join().unwrap();
  
  
  Ok(())
}