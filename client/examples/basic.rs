use std::{time::{Duration, Instant}, sync::Arc};

use client::{Latency, mixer::{self, Mixer}, client::ClientAudioPacket, audio};
use common::packets::{AudioPacket, SeqNum};
use cpal::traits::{HostTrait, StreamTrait, DeviceTrait};
use anyhow::anyhow;
use crossbeam::channel::{self, TryRecvError};
use log::{debug, info, error};
use ringbuf::{HeapProducer, HeapConsumer};

extern crate client;

extern crate env_logger;

fn setup_playback(host: &cpal::Host, latency_ms: f32) -> anyhow::Result<(HeapProducer<f32>, Latency, u32, cpal::Stream)> {
  info!("Playback:");
  let device = host.default_output_device()
    .ok_or_else(|| anyhow!("could not get output device"))?;
  info!(" - Device: {:?}", device.name()?);
  let config: cpal::StreamConfig = device.default_output_config()?.into();
  let config = audio::playback::get_config(&device)?;
  debug!(" - Config: {:?}", config);

  let latency = client::Latency::new(latency_ms, config.sample_rate.0, config.channels);
  info!(" - Channels: {}", config.channels);
  info!(" - Sample Rate: {}", config.sample_rate.0);
  info!(" - Latency: {} samples", latency.samples());

  let (prod, cons) = client::make_buffer(latency).split();
  let stream = audio::playback::make_stream(&device, &config, cons)?;

  stream.play()?;

  Ok((prod, latency, config.sample_rate.0, stream))
}

fn setup_mic(host: &cpal::Host, latency_ms: f32) -> anyhow::Result<(HeapConsumer<f32>, Latency, u32, cpal::Stream)> {
  info!("Playback:");
  let device = host.default_input_device()
    .ok_or_else(|| anyhow!("could not get input device"))?;
  info!(" - Device: {:?}", device.name()?);
  // let config: cpal::StreamConfig = device.default_input_config()?.into();
  let config = audio::microphone::get_config(&device)?;
  debug!(" - Config: {:?}", config);

  let latency = client::Latency::new(latency_ms, config.sample_rate.0, config.channels);
  info!(" - Channels: {}", config.channels);
  info!(" - Sample Rate: {}", config.sample_rate.0);
  info!(" - Latency: {} samples", latency.samples());

  let (prod, cons) = client::make_buffer(latency).split();
  let stream = audio::microphone::make_stream(&device, &config, prod)?;

  stream.play()?;

  Ok((cons, latency, config.sample_rate.0, stream))
}

fn main() -> anyhow::Result<()> {
  env_logger::init();

  let host = cpal::default_host();


  let (mut o_prod, o_latency, o_rate, playback) = setup_playback(&host, 150.)?;
  let (mut i_cons, i_latency, i_rate, mic) = setup_mic(&host, 150.)?;
  
  let (mic_tx, mic_rx) = channel::bounded::<ClientAudioPacket<u8>>(10_000);
  let (peer_tx, peer_rx) = channel::bounded::<AudioPacket<u8>>(10_000);

  let mut client = client::client::Client::new("hi".into(), mic_rx, peer_tx);
  client.connect("127.0.0.1:8080");

  let connect_rx = client.get_peer_connected_rx();

  let client = Arc::new(client);
  let client_handle;
  {
    let client = client.clone();
    client_handle = std::thread::spawn(move|| {
      client.service();
    });
  }
  let mixer = Mixer::new(o_prod);

  std::thread::spawn(move || {
    let mut encoder = client::opus::OpusEncoder::new(i_rate).unwrap();
    let mut buf = vec![0.0; i_latency.samples()];
    let mut seq_num = SeqNum(0);
    loop {
      if i_cons.len() > encoder.frame_size() {
        let bytes = i_cons.pop_slice(&mut buf);
        if bytes > 0 {
          // debug!("pushed {bytes:>3} bytes mic -> speaker");
          if let Some(data) = encoder.push(&buf[..bytes]) {
            mic_tx.try_send(ClientAudioPacket {
              seq_num,
              data,
            }).unwrap();
            seq_num += 1;
          }
          // o_prod.push_slice(&buf[..bytes]);
        }
      }
    }
  });

  std::thread::spawn(move || {
    let mut mixer = mixer;
    mixer.add_peer(0);

    let mut decoder = client::opus::OpusDecoder::new(o_rate).unwrap();

    loop {
      while mixer.tick() {}

      {
        while let Ok(packet) = peer_rx.try_recv() {
          debug!("<- ({}) {} bytes", packet.seq_num, packet.data.len());
          if let Ok(data) = decoder.decode(&packet.data) {
            mixer.push_data(AudioPacket { seq_num: packet.seq_num, peer_id: packet.peer_id, data });
          }
        }
      }
      {
        match connect_rx.try_recv() {
          Ok(info) => {
            mixer.add_peer(info.id as u8);
          },
          Err(e) if e != TryRecvError::Empty => {
            error!("Error receiving peer connections: {}", e);
          },
          _ => {},
        }
      }
    }
  }).join().unwrap();
  
  let mut b = String::new();
  std::io::stdin().read_line(&mut b)?;


  Ok(())
}