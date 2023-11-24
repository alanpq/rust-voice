use std::{
  collections::VecDeque,
  sync::{mpsc::Sender, Arc, Mutex},
};

use common::packets;
use log::{info, warn};

use crate::{
  source::{AudioByteSource, AudioSource},
  util::opus::nearest_opus_rate,
};

pub struct OpusEncoder<S: AudioSource> {
  /// the sample rate of the encoder
  opus_rate: u32,

  source: S,

  encoder: Arc<Mutex<opus::Encoder>>,
  frame_size: usize,
  /// buffer of raw audio data to encode
  in_buffer: Arc<Mutex<VecDeque<f32>>>,
}

impl<S: AudioSource> OpusEncoder<S> {
  pub fn new(source: S) -> Result<Self, anyhow::Error> {
    let opus_rate = nearest_opus_rate(source.sample_rate()).unwrap();
    let frame_size = (opus_rate * 20) as usize / 1000;
    info!(
      "Creating new OpusEncoder with frame size {} @ opus:{} hz (real:{} hz)",
      frame_size,
      opus_rate,
      source.sample_rate()
    );

    if opus_rate != source.sample_rate() {
      warn!("Audio Resampling is not yet supported! Your audio will likely be distorted/pitched.");
    }

    let encoder = opus::Encoder::new(opus_rate, opus::Channels::Mono, opus::Application::Voip)?;
    Ok(Self {
      opus_rate,
      source,
      encoder: Arc::new(Mutex::new(encoder)),
      frame_size,
      in_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(frame_size * 2))),
    })
  }

  pub fn frame_size(&self) -> usize {
    self.frame_size
  }

  pub fn next(&self) -> Option<Vec<u8>> {
    let mut buf = self.in_buffer.lock().unwrap();
    if let Some(s) = self.source.next() {
      buf.push_back(s);
    }
    if buf.len() >= self.frame_size {
      let mut encoder = self.encoder.lock().unwrap();
      let input = buf.drain(..self.frame_size).collect::<Vec<f32>>();
      match encoder.encode_vec_float(&input, packets::PACKET_MAX_SIZE / 2) {
        Ok(packet) => return Some(packet),
        Err(e) => {
          warn!("failed to encode packet: {e:?}");
        }
      }
    }
    None
  }
}

impl<S: AudioSource> AudioByteSource for OpusEncoder<S> {
  fn next(&self) -> Option<Vec<u8>> {
    self.next()
  }
}
