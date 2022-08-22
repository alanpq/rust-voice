use std::{sync::{Arc, Mutex, mpsc::{Sender, Receiver, self}}};
use anyhow::{anyhow, Ok};
use cpal::{traits::{HostTrait, DeviceTrait, StreamTrait}, Stream, BuildStreamError};
use log::{debug, info, error, warn};

use crate::latency::Latency;

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

  output_tx: Sender<f32>,
  output_rx: Arc<Mutex<Receiver<f32>>>,

  mic_tx: Sender<f32>,
  mic_rx: Option<Receiver<f32>>,
}

fn error(err: cpal::StreamError) {
  eprintln!("{}", err);
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

  pub fn get_output_tx(&self) -> Sender<f32> {
    self.output_tx.clone()
  }

  pub fn take_mic_rx(&mut self) -> Option<Receiver<f32>> {
    self.mic_rx.take()
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
    self.input_device.build_input_stream(&self.input_config, data_fn, error)
  }

  fn make_output_stream(&mut self) -> Result<Stream, BuildStreamError> {
    let config = self.output_config.clone();
    let rx = self.output_rx.clone();
    let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
      {
        let rx = rx.lock().unwrap();
        let channels = config.channels as usize;
        for i in 0..data.len()/channels {
          // since currently all input is mono, we must duplicate the sample for every channel
          let sample = rx.try_recv().unwrap_or(0.0);
          for j in 0..channels {
            data[i*channels + j] = sample;
          }
        }
      }
    };
    self.output_device.build_output_stream(&self.output_config, data_fn, error)
  }
}

pub struct AudioServiceBuilder {
  host: cpal::Host,
  output_device: Option<cpal::Device>,
  input_device: Option<cpal::Device>,
  latency_ms: f32,
}

impl AudioServiceBuilder {
  pub fn new() -> Self {
    Self { host: cpal::default_host(), output_device: None, input_device: None, latency_ms: 150.0}
  }

  pub fn with_latency(mut self, latency_ms: f32) -> Self {
    self.latency_ms = latency_ms;
    self
  }

  pub fn build(self) -> Result<AudioService, anyhow::Error> {
    let output_device = self.output_device.unwrap_or(
      self.host.default_output_device().ok_or_else(|| anyhow!("no output device available"))?
    );
    let input_device = self.input_device.unwrap_or(
      self.host.default_input_device().ok_or_else(|| anyhow!("no input device available"))?
    );
    info!("Output device: {:?}", output_device.name()?);
    info!("Input device: {:?}", input_device.name()?);

    let input_config: cpal::StreamConfig = input_device.default_input_config()?.into();
    debug!("Default input config: {:?}", input_config);

    let output_config: cpal::StreamConfig = output_device.default_output_config()?.into();
    debug!("Default output config: {:?}", output_config);

    info!("Input:");
    info!(" - Channels: {}", input_config.channels);
    info!(" - Sample Rate: {}", input_config.sample_rate.0);

    info!("Output:");
    info!(" - Channels: {}", output_config.channels);
    info!(" - Sample Rate: {}", output_config.sample_rate.0);

    let out_latency = Latency::new(self.latency_ms, output_config.sample_rate.0, output_config.channels);
    let mic_latency = Latency::new(self.latency_ms, output_config.sample_rate.0, output_config.channels);

    let (output_tx, output_rx) = mpsc::channel();
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

      output_tx,
      output_rx: Arc::new(Mutex::new(output_rx)),

      mic_tx,
      mic_rx: Some(mic_rx),
    })
  }
}
