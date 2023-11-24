use std::sync::{mpsc, Arc, Mutex};

use log::error;

pub trait AudioByteSource: Send + Sync {
  fn next(&self) -> Option<Vec<u8>>;
}

// TODO: support closing of audio sources
pub trait AudioSource: Send + Sync {
  fn next(&self) -> Option<f32>;

  fn sample_rate(&self) -> u32;
}

pub struct AudioMpsc(Arc<Mutex<mpsc::Receiver<f32>>>, u32);

impl AudioMpsc {
  pub fn new(receiver: mpsc::Receiver<f32>, sample_rate: u32) -> Self {
    Self(Mutex::new(receiver).into(), sample_rate)
  }
}

impl AudioSource for AudioMpsc {
  fn next(&self) -> Option<f32> {
    let rx = self.0.lock().unwrap();
    match rx.recv() {
      Ok(s) => Some(s),
      Err(e) => {
        error!("could not get sample from audio source: {e}");
        None
      }
    }
  }

  fn sample_rate(&self) -> u32 {
    self.1
  }
}
