use std::{sync::{Arc, Mutex, mpsc::channel, atomic::{AtomicBool, Ordering}}, net::SocketAddr};

use audio::AudioService;
use client::Client;
use common::packets::ClientMessage;

mod audio;
mod client;

fn main() -> Result<(), anyhow::Error> {
  env_logger::builder().filter_level(log::LevelFilter::Debug).init();

  let (mic_tx, mic_rx) = channel::<Vec<i16>>();
  let (peer_tx, peer_rx) = channel::<(u32, Vec<i16>)>();

  let mut client = Client::new("test".to_string(), mic_rx, peer_tx);
  client.connect("127.0.0.1:8080");

  let client_arc = Arc::new(Mutex::new(client));

  let handle;
  {
    let client_arc = client_arc.clone();
    handle = std::thread::spawn(move|| {
      client_arc.lock().unwrap().service();
    });
  }

  let audio_running = Arc::new(AtomicBool::new(true));

  {
    let audio_running = audio_running.clone();
    std::thread::spawn(move || {
      let mut audio = AudioService::builder()
        .with_channels(mic_tx, peer_rx)
        .with_latency(140.)
        .build().unwrap();
      audio.start().unwrap();

      while audio_running.load(Ordering::SeqCst) {
        match audio.peer_rx.recv() {
          Ok((peer_id, samples)) => {
            for sample in samples {
              audio.push(sample).unwrap();
            }
          }
          Err(e) => {
            println!("{:?}", e);
            break;
          }
        }
      }

      audio.stop();
    });
  }

  // mutex will be unlocked when client is closed.
  handle.join().unwrap();
  audio_running.store(false, Ordering::SeqCst);
  
  
  Ok(())
}