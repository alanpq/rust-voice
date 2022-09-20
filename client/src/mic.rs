use std::{borrow::BorrowMut, sync::{Mutex, Arc}};

use anyhow::anyhow;
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use log::{info, error};
use ringbuf::{Producer, Consumer, RingBuffer};

use crate::{util::opus::OPUS_SAMPLE_RATES, latency::Latency};

pub struct MicService {
  host: cpal::Host,
  device: cpal::Device,
  config: cpal::StreamConfig,
  stream: Option<cpal::Stream>,
  producer: Arc<Mutex<Producer<f32>>>,
}

fn error(err: cpal::StreamError) {
  error!("{}", err);
}

impl MicService {
  pub fn builder() -> MicServiceBuilder {
    MicServiceBuilder::new()
  }

  pub fn start(&mut self) -> Result<(), anyhow::Error> {
    let producer = self.producer.clone();
    self.stream = Some(self.device.build_input_stream(&self.config, move |data: &[f32], _: &cpal::InputCallbackInfo| {
      let mut producer = producer.lock().unwrap();
      producer.push_slice(data);
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
  pub fn build(self) -> Result<(MicService, Consumer<f32>), anyhow::Error> {
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

    let ring = RingBuffer::new(latency.samples() * 2);
    let (mut producer, consumer) = ring.split();
    for _ in 0..latency.samples() {
      producer.push(0.0).unwrap();
    }

    Ok((MicService {
      host: self.host,
      device,
      config,
      stream: None,
      producer: Arc::new(Mutex::new(producer)),
    }, consumer))
  }
}