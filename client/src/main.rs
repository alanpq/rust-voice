use std::{sync::{Arc, Mutex, mpsc::channel, atomic::{AtomicBool, Ordering}}, net::SocketAddr, time::Duration};
use clap::Parser;

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


  let audio_handle;
  {
    // let client = client_arc.clone();
    audio_handle = std::thread::spawn(move || {
      let mut audio = AudioService::builder()
        .with_latency(args.latency)
        .build().unwrap();

        
      let mixer = PeerMixer::new(
        audio.out_config().sample_rate.0,
        audio.out_latency(),
        audio.get_output_tx()
      );
      audio.start().unwrap();

      let input_consumer = audio.take_mic_rx().expect("microphone tx already taken");
      let mut encoder = OpusEncoder::new(audio.out_config().sample_rate.0).unwrap();
      let mut decoder = OpusDecoder::new(audio.out_config().sample_rate.0).unwrap();
      encoder.add_output(mic_tx);

      let (test_tx, test_rx) = channel::<Vec<u8>>();
      encoder.add_output(test_tx);

      let audio_tx = audio.get_output_tx();

      while client_is_running.load(Ordering::SeqCst) {
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
        if let Ok(packet) = test_rx.try_recv() {
          match decoder.decode(&packet) {
            Ok(samples) => {
              for sample in samples {
                audio_tx.send(sample).unwrap();
              }
            }
            Err(e) => {
              error!("Error decoding test packet: {}", e);
            }
          }
        }
        mixer.tick();
        // std::thread::sleep(Duration::from_millis(200));

        // match .try_recv() {
        //   Ok(packet) => {
        //     client.send(packets::ClientMessage::Voice { samples: packet });
        //   }
        //   Err(e) => {
        //     if e != std::sync::mpsc::TryRecvError::Empty {
        //       error!("Error receiving packet: {}", e);
        //     }
        //   }
        // }
      }

      audio.stop();

    });
  }

  // client.on_peer_connected(|id, name| {
  //   info!("{} has connected.", &name);
  // });

  client_handle.join().unwrap();
  audio_handle.join().unwrap();
  
  
  Ok(())
}