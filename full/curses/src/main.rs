use anyhow::Context as _;
use app::LogPipe;
use clap::Parser;
use std::{
  net::SocketAddr,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex, RwLock,
  },
  thread::JoinHandle,
  time::Duration,
};

use client::{
  audio::AudioHandle, client::Client, mixer::PeerMixer, opus::OpusEncoder, source::AudioMpsc,
};
use common::{
  packets::{self, AudioPacket, ClientMessage},
  UserInfo,
};
use crossbeam::channel::{self, TryRecvError};
use flexi_logger::{Logger, WriteMode};
use log::{error, info};
use tracing::{span, Level};

mod app;

#[derive(Parser, Debug)]
#[clap(name = "Rust Voice Server")]
struct Args {
  #[clap(value_parser)]
  address: String,
  #[clap(value_parser = clap::value_parser!(u16).range(1..), short='p', long="port", default_value_t=8080)]
  port: u16,
  #[clap(value_parser, long = "latency", default_value_t = 150.)]
  latency: f32,
}

struct SharedState {
  pub args: Args,
  pub client_running: AtomicBool,
  pub peer_connect_rx: channel::Receiver<UserInfo>,
}

fn audio_thread(
  mixer: Arc<PeerMixer>,
  state: Arc<SharedState>,
  peer_rx: Receiver<AudioPacket<u8>>,
) -> JoinHandle<()> {
  std::thread::spawn(move || {
    let go = move || {
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
        {
          match state.peer_connect_rx.try_recv() {
            Ok(info) => {
              mixer.add_peer(info.id);
            }
            Err(e) if e != TryRecvError::Empty => {
              error!("Error receiving peer connections: {}", e);
            }
            _ => {}
          }
        }
      }
      Ok::<(), anyhow::Error>(())
    };
    if let Err(e) = go() {
      error!("[Audio] {e:?}");
    }
  })
}

fn main() -> Result<(), anyhow::Error> {
  let pipe = LogPipe::new();
  Logger::try_with_str("debug")?
    .log_to_writer(Box::new(pipe.clone()))
    .write_mode(WriteMode::Async)
    .start()?;

  #[cfg(feature = "trace")]
  {
    use tracing_subscriber::layer::SubscriberExt;

    tracing::subscriber::set_global_default(
      tracing_subscriber::registry().with(tracing_tracy::TracyLayer::new()),
    )
    .expect("set up the subscriber");
  }

  let args = Args::parse();

  let addr = format!("{}:{}", args.address, args.port)
    .parse::<SocketAddr>()
    .expect("Invalid server address.");

  let (peer_tx, peer_rx) = channel::<AudioPacket<u8>>();

  let (mut audio, mic) = AudioHandle::builder().with_latency(args.latency).start()?;

  let mixer = Arc::new(PeerMixer::new(
    audio.out_cfg().sample_rate.0,
    audio.out_latency(),
  ));
  audio.add_source(mixer.clone());

  let mic = OpusEncoder::new(mic).context("failed to create encoder")?;

  let mut client = Client::new("test".to_string(), Arc::new(mic), peer_tx);
  client.connect(addr);
  let peer_connect_rx = client.get_peer_connected_rx();

  let state = Arc::new(SharedState {
    args,
    client_running: AtomicBool::new(true),
    peer_connect_rx,
  });
  audio_thread(mixer, state.clone(), peer_rx);

  let client = Arc::new(client);
  {
    let client = client.clone();
    let state = state.clone();
    std::thread::spawn(move || {
      client.service();
      state.client_running.store(false, Ordering::SeqCst);
    });
  }

  let app_handle = {
    let pipe = pipe;
    std::thread::spawn(move || {
      let mut app = app::App::new(pipe, client);
      app.run().unwrap();
    })
  };

  app_handle.join().unwrap();

  audio.stop();

  Ok(())
}
