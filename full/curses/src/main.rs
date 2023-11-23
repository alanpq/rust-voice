use std::{sync::{Arc, Mutex, mpsc::{channel, Sender, Receiver}, atomic::{AtomicBool, Ordering}, RwLock}, net::SocketAddr, time::Duration, thread::JoinHandle};
use app::LogPipe;
use clap::Parser;

use crossbeam::channel::{TryRecvError, self};
use flexi_logger::{Logger, WriteMode};
use latency::Latency;
use log::{info, error};
use services::{AudioService, PeerMixer, OpusEncoder};
use client::Client;
use common::{packets::{ClientMessage, self, AudioPacket}, UserInfo};
use tracing::{span, Level};

use crate::services::OpusDecoder;

mod services;
mod client;
mod latency;
mod util;
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

struct SharedState {
  pub args: Args,
  pub client_running: AtomicBool,
  pub peer_connect_rx: channel::Receiver<UserInfo>,
}

fn audio_thread(state: Arc<SharedState>, peer_rx: Receiver<AudioPacket<u8>>,mic_tx: Sender<Vec<u8>>) -> JoinHandle<()> {
  std::thread::spawn(move || {
    let go = move || {
        let mut audio = AudioService::builder()
          .with_latency(state.args.latency)
          .build()?;
        let mixer = PeerMixer::new(
          audio.out_config().sample_rate.0,
          audio.out_latency(),
          audio.get_output_tx()
        );
        audio.start()?;

        let input_consumer = audio.take_mic_rx().expect("microphone tx already taken");
        let mut encoder = OpusEncoder::new(audio.out_config().sample_rate.0).expect("failed to create encoder");
        encoder.add_output(mic_tx);
        let span = span!(Level::INFO, "audio_thread");
        while state.client_running.load(Ordering::SeqCst) {
          let _span = span.enter();
          match peer_rx.try_recv() {
            Ok(packet) => {
              mixer.push(packet.peer_id.into(), &packet.data);
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
        Ok::<(), anyhow::Error>(())
    };
    if let Err(e) = go() {
        error!("[Audio] {e:?}");
    }
  })
}

fn main() -> Result<(), anyhow::Error> {
  let pipe = LogPipe::new();
  Logger::try_with_str("info")?
    .log_to_writer(Box::new(pipe.clone()))
    .write_mode(WriteMode::Async)
    .start()?;

  #[cfg(feature="trace")]
  {
    use tracing_subscriber::layer::SubscriberExt;

    tracing::subscriber::set_global_default(
      tracing_subscriber::registry()
          .with(tracing_tracy::TracyLayer::new()),
    ).expect("set up the subscriber");
  }

  let args = Args::parse();

  let addr = format!("{}:{}", args.address, args.port)
    .parse::<SocketAddr>().expect("Invalid server address.");

  let (mic_tx, mic_rx) = channel::<Vec<u8>>();
  let (peer_tx, peer_rx) = channel::<AudioPacket<u8>>();

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
    let client = client.clone();
    let state = state.clone();
    client_handle = std::thread::spawn(move|| {
      client.service();
      state.client_running.store(false, Ordering::SeqCst);
    });
  }

  let audio_handle = audio_thread(state, peer_rx, mic_tx);

  let app_handle = {
    let pipe = pipe;
    std::thread::spawn(move || {
      let mut app = app::App::new(pipe, client);
      app.run();
    })
  };

  app_handle.join().unwrap();
  
  Ok(())
}
