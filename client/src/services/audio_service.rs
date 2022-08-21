use std::{error::Error, sync::{Arc, Mutex, mpsc::{Sender, Receiver, self}, atomic::{AtomicBool, Ordering}}, collections::{HashMap, VecDeque}};
use anyhow::{anyhow, Ok};
use common::packets;
use cpal::{traits::{HostTrait, DeviceTrait, StreamTrait}, InputDevices, InputCallbackInfo, OutputCallbackInfo, Stream, BuildStreamError};
use log::{debug, info, error, warn};
use ringbuf::{RingBuffer, Consumer, Producer};

use crate::latency::Latency;

use super::OpusEncoder;

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
    // {
    //   let mut input = self.input.lock().unwrap();
    //   for _ in 0..self.latency_samples {
    //     input.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
    //   }
    // }
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
    let mut rx = self.output_rx.clone();//.expect("output rx already taken. did you already call make_output_stream?");
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
    let output_device = self.output_device.or(Some(
      self.host.default_output_device().ok_or(anyhow!("no output device available"))?
    )).unwrap();
    let input_device = self.input_device.or(Some(
      self.host.default_input_device().ok_or(anyhow!("no input device available"))?
    )).unwrap();
    info!("Output device: {:?}", output_device.name()?);
    info!("Input device: {:?}", input_device.name()?);

    let input_config: cpal::StreamConfig = input_device.default_input_config()?.into();
    let input_sample_rate = input_config.sample_rate.0;
    debug!("Default input config: {:?}", input_config);

    let output_config: cpal::StreamConfig = output_device.default_output_config()?.into();
    let output_sample_rate = output_config.sample_rate.0;
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
