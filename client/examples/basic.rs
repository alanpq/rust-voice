use std::time::Duration;

use client::Latency;
use cpal::traits::{HostTrait, StreamTrait, DeviceTrait};
use anyhow::anyhow;
use log::{debug, info, error};
use ringbuf::{HeapProducer, HeapConsumer};

extern crate client;

extern crate env_logger;

fn setup_playback(host: &cpal::Host, latency_ms: f32) -> anyhow::Result<(HeapProducer<f32>, Latency, cpal::Stream)> {
  info!("Playback:");
  let device = host.default_output_device()
    .ok_or_else(|| anyhow!("could not get output device"))?;
  info!(" - Device: {:?}", device.name()?);
  let config: cpal::StreamConfig = device.default_output_config()?.into();
  let config = client::playback::get_config(&device)?;
  debug!(" - Config: {:?}", config);

  let latency = client::Latency::new(latency_ms, config.sample_rate.0, config.channels);
  info!(" - Channels: {}", config.channels);
  info!(" - Sample Rate: {}", config.sample_rate.0);
  info!(" - Latency: {} samples", latency.samples());

  let (prod, cons) = client::make_buffer(latency).split();
  let stream = client::playback::make_stream(&device, &config, cons)?;

  stream.play()?;

  Ok((prod, latency, stream))
}

fn setup_mic(host: &cpal::Host, latency_ms: f32) -> anyhow::Result<(HeapConsumer<f32>, Latency, cpal::Stream)> {
  info!("Playback:");
  let device = host.default_input_device()
    .ok_or_else(|| anyhow!("could not get input device"))?;
  info!(" - Device: {:?}", device.name()?);
  // let config: cpal::StreamConfig = device.default_input_config()?.into();
  let config = client::microphone::get_config(&device)?;
  debug!(" - Config: {:?}", config);

  let latency = client::Latency::new(latency_ms, config.sample_rate.0, config.channels);
  info!(" - Channels: {}", config.channels);
  info!(" - Sample Rate: {}", config.sample_rate.0);
  info!(" - Latency: {} samples", latency.samples());

  let (prod, cons) = client::make_buffer(latency).split();
  let stream = client::microphone::make_stream(&device, &config, prod)?;

  stream.play()?;

  Ok((cons, latency, stream))
}

fn main() -> anyhow::Result<()> {
  env_logger::init();

  let host = cpal::default_host();

  let (mut o_prod, o_latency, playback) = setup_playback(&host, 150.).unwrap();
  let (mut i_cons, i_latency, mic) = setup_mic(&host, 150.)?;


  std::thread::spawn(move || {
    let mut buf = vec![0.0; i_latency.samples()];
    loop {
      let bytes = i_cons.pop_slice(&mut buf);
      if bytes > 0 {
        // debug!("pushed {bytes:>3} bytes mic -> speaker");
        o_prod.push_slice(&buf[..bytes]);
      }
    }
  }).join().unwrap();
  
  let mut b = String::new();
  std::io::stdin().read_line(&mut b)?;


  Ok(())
}