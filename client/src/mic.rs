use std::{borrow::BorrowMut, sync::{Mutex, Arc, mpsc::{Sender, Receiver}}, collections::VecDeque};

use anyhow::anyhow;
use common::packets;
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use log::{info, error, warn};
use ringbuf::{Producer, Consumer, RingBuffer};

use crate::{util::opus::{OPUS_SAMPLE_RATES, nearest_opus_rate}, latency::Latency};

pub struct MicService {
  host: cpal::Host,
  device: cpal::Device,
  config: cpal::StreamConfig,
  stream: Option<cpal::Stream>,
  latency: Latency,
  
  frame_size: usize,
  tx: Arc<Mutex<Sender<Vec<u8>>>>,
  encoder: Arc<Mutex<opus::Encoder>>,
  buffer: Arc<Mutex<VecDeque<f32>>>,
}

fn error(err: cpal::StreamError) {
  error!("{}", err);
}

impl MicService {
  pub fn builder() -> MicServiceBuilder {
    MicServiceBuilder::new()
  }

  pub fn latency(&self) -> Latency {
    self.latency
  }

  pub fn start(&mut self) -> Result<(), anyhow::Error> {
    // let producer = self.producer.clone();
    let encoder = self.encoder.clone();
    let buffer = self.buffer.clone();
    let frame_size = self.frame_size;
    let tx = self.tx.clone();
    self.stream = Some(self.device.build_input_stream(&self.config, move |data: &[f32], _: &cpal::InputCallbackInfo| {
      let mut buffer = buffer.lock().unwrap();
      for sample in data {
        buffer.push_back(*sample);
      }
      if buffer.len() >= frame_size {
        let mut encoder = encoder.lock().unwrap();
        let input = buffer.drain(..frame_size).collect::<Vec<f32>>();
        match encoder.encode_vec_float(&input, packets::PACKET_MAX_SIZE/2) {
          Ok(packet) => {
            let tx = tx.lock().unwrap();
            tx.send(packet);
          },
          Err(e) => {
            warn!("Failed to encode audio: {}", e);
          }
        }
      }
    }, error)?);
    self.stream.as_ref().unwrap().play()?;
    Ok(())
  }
}



pub struct MicServiceBuilder {
  host: cpal::Host,
  device: Option<cpal::Device>,
  latency_ms: f32,
}

impl MicServiceBuilder {
  pub fn new() -> Self {
    Self { host: cpal::default_host(), device: None, latency_ms: 150.0 }
  }
  pub fn with_latency(mut self, latency_ms: f32) -> Self {
    self.latency_ms = latency_ms;
    self
  }
  pub fn build(self) -> Result<(MicService, Receiver<Vec<u8>>), anyhow::Error> {
    let device = self.device.unwrap_or(
      self.host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?
    );
    info!("Input device: {:?}", device.name()?);
    let config: cpal::StreamConfig = match device.supported_input_configs() {
      Result::Ok(configs) => {
        let mut out = None;
        for config in configs {
          if out.is_some() { break; }
          for rate in OPUS_SAMPLE_RATES {
            if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
              out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
              break;
            }
          }
        }
        out
      }
      Err(_) => None
    }.unwrap_or(device.default_input_config()?.into());

    let latency = Latency::new(self.latency_ms, config.sample_rate.0, config.channels);
    
    info!("Input:");
    info!(" - Channels: {}", config.channels);
    info!(" - Sample Rate: {}", config.sample_rate.0);

    let ring = RingBuffer::new(latency.samples() * 2);
    let (mut producer, consumer) = ring.split();
    for _ in 0..latency.samples() {
      producer.push(0).unwrap();
    }

    let opus_rate = nearest_opus_rate(config.sample_rate.0).unwrap();
    let frame_size = (opus_rate * 20) as usize / 1000;
    info!("Creating new OpusEncoder with frame size {} @ opus:{} hz (real:{} hz)", frame_size, opus_rate, config.sample_rate.0);
    
    if opus_rate != config.sample_rate.0 {
      warn!("Audio Resampling is not yet supported! Your audio will likely be distorted/pitched.");
    }
    let encoder = opus::Encoder::new(opus_rate, opus::Channels::Mono, opus::Application::Voip)?;

    let (tx, rx) = std::sync::mpsc::channel();

    Ok((MicService {
      host: self.host,
      device,
      config,
      stream: None,
      latency,
      tx: Arc::new(Mutex::new(tx)),
      buffer: Arc::new(Mutex::new(VecDeque::new())),
      encoder: Arc::new(Mutex::new(encoder)),
      frame_size,
    }, rx))
  }
}