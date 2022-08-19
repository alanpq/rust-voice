use std::{error::Error, sync::{Arc, Mutex, mpsc::{Sender, Receiver}}};
use anyhow::{anyhow, Ok};
use common::packets;
use cpal::{traits::{HostTrait, DeviceTrait, StreamTrait}, InputDevices, InputCallbackInfo, OutputCallbackInfo, Stream, BuildStreamError};
use log::{debug, info};
use ringbuf::{RingBuffer, Consumer, Producer};

pub struct Input<T> {
  producer: Producer<T>
}

impl<T> Input<T> {
  pub fn push(&mut self, sample: T) -> Result<(), T> {
    self.producer.push(sample)
  }
}

pub struct Output<T> {
  consumer: Consumer<T>
}

impl<T> Output<T> {
  pub fn pop(&mut self) -> Option<T> {
    self.consumer.pop()
  }
}

pub struct AudioService {
  host: cpal::Host,
  output_device: cpal::Device,
  input_device: cpal::Device,

  input_config: cpal::StreamConfig,
  output_config: cpal::StreamConfig,

  latency_ms: f32,
  latency_frames: f32,
  latency_samples: usize,
  input: Arc<Mutex<Input<f32>>>,
  output: Arc<Mutex<Output<f32>>>,

  input_stream: Option<Stream>,
  output_stream: Option<Stream>,

  pub mic_tx: Sender<Vec<i16>>,
  pub peer_rx: Receiver<(u32, Vec<i16>)>,

  running: bool,
}

fn error(err: cpal::StreamError) {
  eprintln!("{}", err);
}

impl AudioService {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn start(&mut self) -> Result<(), anyhow::Error> {
    if self.running {
      return Ok(());
    }
    {
      let mut input = self.input.lock().unwrap();
      for _ in 0..self.latency_samples {
        input.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
      }
    }

    self.input_stream = Some(self.make_input_stream()?);
    self.output_stream = Some(self.make_output_stream()?);
    self.input_stream.as_ref().unwrap().play()?;
    self.output_stream.as_ref().unwrap().play()?;
    self.running = true;
    Ok(())
  }

  pub fn push(&mut self, sample: i16) -> Result<(), anyhow::Error> {
    let mut input = self.input.lock().unwrap();
    input.push((sample as f32 / i16::MAX as f32)).unwrap();
    Ok(())
  }

  pub fn stop(&mut self) {
    if !self.running {
      return;
    }
    self.running = false;
    drop(self.input_stream.take());
    drop(self.output_stream.take());
  }

  fn make_input_stream(&mut self) -> Result<Stream, BuildStreamError> {
    let mic_tx = self.mic_tx.clone();
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
      {
        let out: Vec<i16> = data.iter().map(|sample| {
          (*sample * i16::MAX as f32) as i16
        }).collect();
        out.chunks((packets::PACKET_MAX_SIZE / 2) - 12).for_each(|chunk| {
          mic_tx.send(chunk.to_vec()).unwrap();
        });
      }
    };
    self.input_device.build_input_stream(&self.input_config, data_fn, error)
  }

  fn make_output_stream(&mut self) -> Result<Stream, BuildStreamError> {
    let output = self.output.clone();
    let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
      let mut input_fell_behind = false;
      {
        let mut output = output.lock().unwrap();
        for sample in data {
          *sample = match output.pop() {
            Some(s) => s,
            None => {
              input_fell_behind = true;
              0.0
            }
          };
        }
      }
      // if input_fell_behind {
      //   log::error!("input stream fell behind: try increasing latency");
      // }
    };
    self.output_device.build_output_stream(&self.output_config, data_fn, error)
  }
}

pub struct AudioServiceBuilder {
  host: cpal::Host,
  output_device: Option<cpal::Device>,
  input_device: Option<cpal::Device>,
  latency_ms: f32,

  mic_tx: Option<Sender<Vec<i16>>>,
  peer_rx: Option<Receiver<(u32, Vec<i16>)>>,
}

impl AudioServiceBuilder {
  pub fn new() -> Self {
    Self { host: cpal::default_host(), output_device: None, input_device: None, latency_ms: 150.0, mic_tx: None, peer_rx: None }
  }

  pub fn with_channels(mut self, mic_tx: Sender<Vec<i16>>, peer_rx: Receiver<(u32, Vec<i16>)>) -> Self {
    self.mic_tx = Some(mic_tx);
    self.peer_rx = Some(peer_rx);
    self
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
    debug!("Output device: {:?}", output_device.name()?);
    debug!("Input device: {:?}", input_device.name()?);

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

    let latency_frames = (self.latency_ms / 1000.) * output_config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * output_config.channels as usize;

    let buffer = RingBuffer::new(latency_samples * 2);
    let (producer, consumer) = buffer.split();


    Ok(AudioService {
      host: self.host,
      output_device,
      input_device,
      input_config,
      output_config,
      latency_ms: self.latency_ms,
      latency_frames,
      latency_samples,
      input: Arc::new(Mutex::new(Input { producer })),
      output: Arc::new(Mutex::new(Output { consumer })),
      input_stream: None,
      output_stream: None,
      running: false,
      mic_tx: self.mic_tx.unwrap(),
      peer_rx: self.peer_rx.unwrap(),
    })
  }
}
