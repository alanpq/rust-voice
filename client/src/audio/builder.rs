use std::sync::{mpsc, Arc, Mutex};

use anyhow::Context;
use cpal::traits::{DeviceTrait as _, HostTrait as _};
use log::{debug, info};

use crate::{
  audio::{streams, AudioService, Statistics},
  opus::OPUS_SAMPLE_RATES,
  source::{AudioMpsc, AudioSource},
  Latency,
};

use super::AudioHandle;

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

  pub fn start(self) -> Result<(AudioHandle, AudioMpsc), anyhow::Error> {
    let output_device = self.output_device.unwrap_or(
      self
        .host
        .default_output_device()
        .context("no output device available")?,
    );
    let input_device = self.input_device.unwrap_or(
      self
        .host
        .default_input_device()
        .context("no input device available")?,
    );
    info!("Output device: {:?}", output_device.name()?);
    info!("Input device: {:?}", input_device.name()?);

    let in_config: cpal::StreamConfig = match input_device.supported_input_configs() {
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

    debug!("Default input config: {:?}", in_config);

    let out_config: cpal::StreamConfig = match output_device.supported_output_configs() {
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
    debug!("Default output config: {:?}", out_config);

    info!("Input:");
    info!(" - Channels: {}", in_config.channels);
    info!(" - Sample Rate: {}", in_config.sample_rate.0);

    info!("Output:");
    info!(" - Channels: {}", out_config.channels);
    info!(" - Sample Rate: {}", out_config.sample_rate.0);

    let out_latency = Latency::new(
      self.latency_ms,
      out_config.sample_rate.0,
      out_config.channels,
    );
    let in_latency = Latency::new(
      self.latency_ms,
      out_config.sample_rate.0,
      out_config.channels,
    );

    let (mic_tx, mic_rx) = futures::channel::mpsc::channel(4096);

    let sources = Arc::new(Mutex::new(self.sources));

    let mic = AudioMpsc::new(mic_rx, in_config.sample_rate.0);

    let stats = Arc::new(Statistics::new());

    let (tx, rx) = mpsc::channel();

    {
      let sources = sources.clone();
      let in_config = in_config.clone();
      let out_config = out_config.clone();
      let stats = stats.clone();
      std::thread::spawn(move || {
        let input_stream =
          streams::make_input_stream(input_device, in_config, mic_tx, stats.clone()).unwrap();
        let output_stream =
          streams::make_output_stream(output_device, out_config, sources, stats).unwrap();
        let service = AudioService {
          input_stream,
          output_stream,
          rx,
        };
        service.run();
      });
    }

    Ok((
      AudioHandle {
        sources,
        out_latency,
        out_config,
        in_latency,
        in_config,
        tx,
        stats,
      },
      mic,
    ))
  }
}

impl Default for AudioServiceBuilder {
  fn default() -> Self {
    Self::new()
  }
}
