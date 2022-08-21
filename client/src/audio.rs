use std::{error::Error, sync::{Arc, Mutex, mpsc::{Sender, Receiver}}, collections::HashMap};
use anyhow::{anyhow, Ok};
use common::packets;
use cpal::{traits::{HostTrait, DeviceTrait, StreamTrait}, InputDevices, InputCallbackInfo, OutputCallbackInfo, Stream, BuildStreamError};
use log::{debug, info};
use ringbuf::{RingBuffer, Consumer, Producer};

pub struct AudioService {
  host: cpal::Host,
  output_device: cpal::Device,
  input_device: cpal::Device,

  input_config: cpal::StreamConfig,
  output_config: cpal::StreamConfig,

  latency_ms: f32,
  latency_frames: f32,
  latency_samples: usize,
  
  peer_buffers_rx: Arc<Mutex<HashMap<u32, Consumer<f32>>>>,
  peer_buffers_tx: Arc<Mutex<HashMap<u32, Producer<f32>>>>,

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
    self.running = true;
    Ok(())
  }

  fn create_peer_buffer(&mut self, peer: u32) {
    let mut buf = RingBuffer::new(self.latency_samples*2);
    let (mut producer, consumer) = buf.split();
    for _ in 0..self.latency_samples {
      producer.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
    }
    {
      self.peer_buffers_tx.lock().unwrap().insert(peer, producer);
      self.peer_buffers_rx.lock().unwrap().insert(peer, consumer);
    }
  }

  pub fn push(&mut self, peer: u32, sample: i16) -> Result<(), anyhow::Error> {
    {
      let input = self.peer_buffers_tx.lock().unwrap();
      if !input.contains_key(&peer) {
        drop(input); // cheeky borrow bypass
        self.create_peer_buffer(peer);
      }
    }
    {
      let mut input = self.peer_buffers_tx.lock().unwrap();
      input.get_mut(&peer).unwrap().push((sample as f32 / i16::MAX as f32));
    }
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
    let config = self.input_config.clone();
    let mic_tx = self.mic_tx.clone();
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
      {
        let out: Vec<i16> = data.iter().step_by(config.channels as usize).map(|sample| {
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
    let config = self.output_config.clone();
    let rx = self.peer_buffers_rx.clone();
    let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
      {
        let mut rx = rx.lock().unwrap();
        // fill each sample of the output buffer
        for i in (0..data.len() / config.channels as usize) {
          let mut final_sample = 0.0;
          // sum each peer's sample
          for (_peer, buf) in rx.iter_mut() {
            final_sample += buf.pop().unwrap_or(0.0);
          }
          
          // since currently all input is mono, we must duplicate the sample for every channel
          for j in 0..config.channels as usize {
            data[(i * config.channels as usize)+j] = final_sample;
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

    let latency_frames = (self.latency_ms / 1000.) * output_config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * output_config.channels as usize;

    Ok(AudioService {
      host: self.host,
      output_device,
      input_device,
      input_config,
      output_config,
      latency_ms: self.latency_ms,
      latency_frames,
      latency_samples,
      
      peer_buffers_rx: Arc::new(Mutex::new(HashMap::new())),
      peer_buffers_tx: Arc::new(Mutex::new(HashMap::new())),

      input_stream: None,
      output_stream: None,
      running: false,
      mic_tx: self.mic_tx.unwrap(),
      peer_rx: self.peer_rx.unwrap(),
    })
  }
}
