use std::{collections::VecDeque, sync::{Mutex, Arc, mpsc::{Sender, self}}};

use common::packets;
use log::{warn, info};

pub struct OpusEncoder {
  encoder: Arc<Mutex<opus::Encoder>>,
  frame_size: usize,
  /// buffer of raw audio data to encode
  buffer: Arc<Mutex<VecDeque<f32>>>,

  tx: Vec<Sender<Vec<u8>>>,
  
}



impl OpusEncoder {
  pub fn new(sample_rate: u32) -> Result<Self, anyhow::Error> {
    let encoder = opus::Encoder::new(sample_rate, opus::Channels::Mono, opus::Application::Voip)?;

    let frame_size = (sample_rate * 20) as usize / 1000;

    info!("Created new OpusEncoder with frame_size {} @ {} hz", frame_size, sample_rate);
    Ok(Self {
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