use std::{collections::VecDeque, sync::{Mutex, Arc, mpsc::Sender}};

use common::packets;
use log::{warn, info};

use crate::util::opus::nearest_opus_rate;

pub struct OpusEncoder {
  /// the real sample rate of the input
  sample_rate: u32,
  /// the sample rate of the encoder
  opus_rate: u32,

  encoder: Arc<Mutex<opus::Encoder>>,
  frame_size: usize,
  /// buffer of raw audio data to encode
  buffer: Arc<Mutex<VecDeque<f32>>>,

  tx: Vec<Sender<Vec<u8>>>,
}

impl OpusEncoder {
  pub fn new(sample_rate: u32) -> Result<Self, anyhow::Error> {
    let opus_rate = nearest_opus_rate(sample_rate).unwrap();
    let frame_size = (opus_rate * 20) as usize / 1000;
    info!("Creating new OpusEncoder with frame size {} @ opus:{} hz (real:{} hz)", frame_size, opus_rate, sample_rate);

    let encoder = opus::Encoder::new(opus_rate, opus::Channels::Mono, opus::Application::Voip)?;
    Ok(Self {
      opus_rate,
      sample_rate,
      encoder: Arc::new(Mutex::new(encoder)),
      frame_size,
      buffer: Arc::new(Mutex::new(VecDeque::with_capacity(frame_size*2))),
      tx: Vec::new(),
    })
  }

  pub fn frame_size(&self) -> usize {
    self.frame_size
  }

  pub fn add_output(&mut self, tx: Sender<Vec<u8>>) {
    self.tx.push(tx);
  }

  pub fn push(&mut self, sample: f32) {
    let mut buffer = self.buffer.lock().unwrap();
    buffer.push_back(sample);

    if buffer.len() >= self.frame_size {
      let mut encoder = self.encoder.lock().unwrap();
      let input = buffer.drain(..self.frame_size).collect::<Vec<f32>>();
      match encoder.encode_vec_float(&input, packets::PACKET_MAX_SIZE/2) {
        Ok(packet) => {
          self.tx.iter().for_each(|tx| tx.send(packet.clone()).unwrap());
        }
        Err(e) => {
          warn!("failed to encode packet: {:?}", e);
        }
      }
    }
  }
}