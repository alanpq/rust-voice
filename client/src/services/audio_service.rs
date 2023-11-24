use anyhow::{anyhow, Context as _, Ok};
use cpal::{
  traits::{DeviceTrait, HostTrait, StreamTrait},
  BuildStreamError, Stream,
};
use log::{debug, error, info, warn};
use std::sync::{
  mpsc::{self, Receiver, Sender},
  Arc, Mutex,
};

use crate::{
  latency::Latency,
  source::{AudioMpsc, AudioSource},
  util::opus::OPUS_SAMPLE_RATES,
};

pub struct AudioService {
  host: cpal::Host,
  output_device: cpal::Device,
  input_device: cpal::Device,

  input_config: cpal::StreamConfig,
  output_config: cpal::StreamConfig,

  mic_latency: Latency,
  out_latency: Latency,

  input_stream: Option<Stream>,
  output_stream: Option<Stream>,

  sources: Arc<Mutex<Vec<Arc<dyn AudioSource>>>>,

  mic_tx: Sender<f32>,
  mic_rx: Option<Receiver<f32>>,
}

fn error(err: cpal::StreamError) {
  error!("{}", err);
}

impl AudioService {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn mic_latency(&self) -> Latency {
    self.mic_latency
  }

  pub fn out_latency(&self) -> Latency {
    self.out_latency
  }

  pub fn out_config(&self) -> &cpal::StreamConfig {
    &self.output_config
  }

  pub fn start(&mut self) -> Result<(), anyhow::Error> {
    self.input_stream = Some(self.make_input_stream()?);
    self.output_stream = Some(self.make_output_stream()?);
    self.input_stream.as_ref().unwrap().play()?;
    self.output_stream.as_ref().unwrap().play()?;

    Ok(())
  }

  pub fn take_mic(&mut self) -> Option<AudioMpsc> {
    self
      .mic_rx
      .take()
      .map(|rx| AudioMpsc::new(rx, self.input_config.sample_rate.0))
  }

  pub fn add_source(&self, source: Arc<dyn AudioSource>) {
    self.sources.lock().unwrap().push(source)
  }

  pub fn stop(&mut self) {
    drop(self.input_stream.take());
    drop(self.output_stream.take());
  }

  fn make_input_stream(&self) -> Result<Stream, BuildStreamError> {
    let mic_tx = self.mic_tx.clone();
    let config = self.input_config.clone();
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
      for sample in data.iter().step_by(config.channels as usize) {
        if let Err(e) = mic_tx.send(*sample) {
          warn!("failed to send mic data to mic_tx: {:?}", e);
        }
      }
    };
    self
      .input_device
      .build_input_stream(&self.input_config, data_fn, error, None)
  }

  fn make_output_stream(&mut self) -> Result<Stream, BuildStreamError> {
    let config = self.output_config.clone();
    let sources = self.sources.clone();
    let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
      {
        let channels = config.channels as usize;
        for i in 0..data.len() / channels {
          let sample = sources
            .lock()
            .unwrap()
            .iter()
            .filter_map(|s| s.next())
            .sum();
          // since currently all input is mono, we must duplicate the sample for every channel
          for j in 0..channels {
            data[i * channels + j] = sample;
          }
        }
      }
    };
    self
      .output_device
      .build_output_stream(&self.output_config, data_fn, error, None)
  }
}

pub struct AudioServiceBuilder {
  host: cpal::Host,
  output_device: Option<cpal::Device>,
  input_device: Option<cpal::Device>,
  latency_ms: f32,
  sources: Vec<Arc<dyn AudioSource>>,
}

impl AudioServiceBuilder {
  pub fn new() -> Self {
    Self {
      host: cpal::default_host(),
      output_device: None,
      input_device: None,
      latency_ms: 150.0,
      sources: Vec::new(),
    }
  }

  pub fn with_source(mut self, source: Arc<dyn AudioSource>) -> Self {
    self.sources.push(source);
    self
  }

  pub fn with_latency(mut self, latency_ms: f32) -> Self {
    self.latency_ms = latency_ms;
    self
  }

  pub fn build(self) -> Result<AudioService, anyhow::Error> {
    let output_device = self.output_device.unwrap_or(
      self
        .host
        .default_output_device()
        .ok_or_else(|| anyhow!("no output device available"))?,
    );
    let input_device = self.input_device.unwrap_or(
      self
        .host
        .default_input_device()
        .ok_or_else(|| anyhow!("no input device available"))?,
    );
    info!("Output device: {:?}", output_device.name()?);
    info!("Input device: {:?}", input_device.name()?);

    let input_config: cpal::StreamConfig = match input_device.supported_input_configs() {
      Result::Ok(configs) => {
        let mut out = None;
        for config in configs {
          if out.is_some() {
            break;
          }
          for rate in OPUS_SAMPLE_RATES {
            if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
              out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
              break;
            }
          }
        }
        out
      }
      Err(_) => None,
    }
    .unwrap_or_else(|| {
      input_device
        .default_input_config()
        .expect("could not get default input config")
        .into()
    });

    debug!("Default input config: {:?}", input_config);

    let output_config: cpal::StreamConfig = match output_device.supported_output_configs() {
      Result::Ok(configs) => {
        let mut out = None;
        for config in configs {
          if out.is_some() {
            break;
          }
          for rate in OPUS_SAMPLE_RATES {
            if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
              out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
              break;
            }
          }
        }
        out
      }
      Err(_) => None,
    }
    .unwrap_or_else(|| {
      output_device
        .default_output_config()
        .expect("could not get default output config")
        .into()
    });
    debug!("Default output config: {:?}", output_config);

    info!("Input:");
    info!(" - Channels: {}", input_config.channels);
    info!(" - Sample Rate: {}", input_config.sample_rate.0);

    info!("Output:");
    info!(" - Channels: {}", output_config.channels);
    info!(" - Sample Rate: {}", output_config.sample_rate.0);

    let out_latency = Latency::new(
      self.latency_ms,
      output_config.sample_rate.0,
      output_config.channels,
    );
    let mic_latency = Latency::new(
      self.latency_ms,
      output_config.sample_rate.0,
      output_config.channels,
    );

    let (mic_tx, mic_rx) = mpsc::channel();

    Ok(AudioService {
      host: self.host,
      output_device,
      input_device,
      input_config,
      output_config,

      mic_latency,
      out_latency,

      input_stream: None,
      output_stream: None,

      sources: Mutex::new(self.sources).into(),

      mic_tx,
      mic_rx: Some(mic_rx),
    })
  }
}
