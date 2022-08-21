use std::{error::Error, sync::{Arc, Mutex, mpsc::{Sender, Receiver}, atomic::{AtomicBool, Ordering}}, collections::{HashMap, VecDeque}};
use anyhow::{anyhow, Ok};
use common::packets;
use cpal::{traits::{HostTrait, DeviceTrait, StreamTrait}, InputDevices, InputCallbackInfo, OutputCallbackInfo, Stream, BuildStreamError};
use log::{debug, info, error, warn};
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
  peer_decoders: Arc<Mutex<HashMap<u32, opus::Decoder>>>,
  decoder_frame_size: usize,

  input_stream: Option<Stream>,
  raw_input_buffer: Arc<Mutex<VecDeque<f32>>>,
  output_stream: Option<Stream>,

  pub mic_tx: Sender<Vec<u8>>,
  peer_rx: Arc<Mutex<Receiver<(u32, Vec<u8>)>>>,

  encoder: Arc<Mutex<opus::Encoder>>,
  encoder_frame_size: usize,

  running: Arc<AtomicBool>,
}

fn error(err: cpal::StreamError) {
  eprintln!("{}", err);
}

impl AudioService {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn start(&mut self) -> Result<(), anyhow::Error> {
    if self.running.load(Ordering::SeqCst) {
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
    self.running.store(true, Ordering::SeqCst);

    self.decoder();
    Ok(())
  }

  pub fn stop(&mut self) {
    if self.running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
      return;
    }
    drop(self.input_stream.take());
    drop(self.output_stream.take());
  }

  fn make_input_stream(&mut self) -> Result<Stream, BuildStreamError> {
    let config = self.input_config.clone();
    let mic_tx = self.mic_tx.clone();
    let encoder = self.encoder.clone();
    let encoder_frame_size = self.encoder_frame_size;
    let raw_input_buffer = self.raw_input_buffer.clone();
    let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
      {
        let mut raw_input_buffer = raw_input_buffer.lock().unwrap();
        let mut encoder = encoder.lock().unwrap();
        for i in (0..data.len()).step_by(config.channels as usize) {          
          raw_input_buffer.push_back(data[i]);
        }

        while raw_input_buffer.len() >= encoder_frame_size {
          let in_buf = raw_input_buffer.drain(..encoder_frame_size).collect::<Vec<f32>>();
          let encoded = encoder.encode_vec_float(&in_buf, packets::PACKET_MAX_SIZE / 2).unwrap();
          mic_tx.send(encoded).unwrap();
        }

        // mic_tx.send(encoded).unwrap();
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
        for i in 0..data.len()/config.channels as usize {
          let mut final_sample = 0.0;
          // sum each peer's sample
          for (_peer, buf) in rx.iter_mut() {
            final_sample += buf.pop().unwrap_or(0.0);
          }
          
          // since currently all input is mono, we must duplicate the sample for every channel
          for j in 0..config.channels as usize {
            data[(i*config.channels as usize) + j] = final_sample;
          }
        }
      }
    };
    self.output_device.build_output_stream(&self.output_config, data_fn, error)
  }

  fn decoder(&mut self) {
    let config = self.output_config.clone();
    let peer_decoders = self.peer_decoders.clone();
    let peer_rx = self.peer_rx.clone();
    let peer_buffers_tx = self.peer_buffers_tx.clone();
    let peer_buffers_rx = self.peer_buffers_rx.clone();
    let running = self.running.clone();
    let decoder_frame_size = self.decoder_frame_size;
    let latency_samples = self.latency_samples;
    std::thread::spawn(move || {
      while running.load(Ordering::SeqCst) {
        let peer_rx = peer_rx.lock().unwrap();
        match peer_rx.recv() {
            Result::Ok((peer, packet)) => {
              let mut decoders = peer_decoders.lock().unwrap();
              let decoder = decoders.entry(peer).or_insert_with(|| {
                opus::Decoder::new(config.sample_rate.0, opus::Channels::Mono).unwrap()
              });
              let mut output = vec![0.0; (((config.sample_rate.0 * 120) / 1000) * config.channels as u32) as usize];
              match decoder.decode_float(&packet, &mut output[..], false) {
                Result::Ok(samples) => {
                  let mut pb_tx = peer_buffers_tx.lock().unwrap();
                  if !pb_tx.contains_key(&peer) {
                    let buf = RingBuffer::new(latency_samples*2);
                    let (mut producer, consumer) = buf.split();
                    for _ in 0..latency_samples {
                      producer.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
                    }
                    pb_tx.insert(peer, producer);
                    peer_buffers_rx.lock().unwrap().insert(peer, consumer);
                  }
                  let tx = pb_tx.get_mut(&peer).expect(format!("peer buffer tx not found for peer {}", peer).as_str());
                  for i in 0..samples {
                    if let Result::Err(_) = tx.push(output[i]) {
                      warn!("failed to push decoded frame to peer buffer (peer {})", peer);
                    }
                  }
                },
                Err(e) => {
                  println!("decoder error: {}", e);
                  println!("decoder frame size: {}", decoder_frame_size);
                  println!("  -> * {} channels = {}", config.channels, config.channels as usize * decoder_frame_size);
                  println!("frame len: {}", packet.len());
                }
              }
            },
            Result::Err(e) => {
              println!("{:?}", e);
              break;
            },
        }
      }
    });
  }
}

pub struct AudioServiceBuilder {
  host: cpal::Host,
  output_device: Option<cpal::Device>,
  input_device: Option<cpal::Device>,
  latency_ms: f32,

  mic_tx: Option<Sender<Vec<u8>>>,
  peer_rx: Option<Receiver<(u32, Vec<u8>)>>,
}

impl AudioServiceBuilder {
  pub fn new() -> Self {
    Self { host: cpal::default_host(), output_device: None, input_device: None, latency_ms: 150.0, mic_tx: None, peer_rx: None }
  }

  pub fn with_channels(mut self, mic_tx: Sender<Vec<u8>>, peer_rx: Receiver<(u32, Vec<u8>)>) -> Self {
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

    let latency_frames = (self.latency_ms / 1000.) * output_config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * output_config.channels as usize;

    let encoder_frame_size = (input_sample_rate * 20) as usize / 1000;

    info!("Encoder:");
    info!(" - Frame Size: {}", encoder_frame_size);

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
      peer_decoders: Arc::new(Mutex::new(HashMap::new())),

      decoder_frame_size: (output_sample_rate * 20) as usize / 1000,
      encoder_frame_size,

      input_stream: None,
      raw_input_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(encoder_frame_size*2))),
      output_stream: None,
      running: Arc::new(AtomicBool::new(false)),
      mic_tx: self.mic_tx.unwrap(),
      peer_rx: Arc::new(Mutex::new(self.peer_rx.unwrap())),

      encoder: Arc::new(Mutex::new(opus::Encoder::new(input_sample_rate, opus::Channels::Mono, opus::Application::Voip).unwrap())),
    })
  }
}
