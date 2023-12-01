use std::sync::Arc;

use async_trait::async_trait;
use futures::{channel::mpsc, lock::Mutex, StreamExt as _};

#[async_trait]
pub trait AudioByteSource: Send + Sync {
  async fn next(&self) -> Option<Vec<u8>>;
}

// TODO: support closing of audio sources
#[async_trait]
pub trait AudioSource: Send + Sync {
  async fn next(&self) -> Option<f32>;

  fn sample_rate(&self) -> u32;
}

pub struct AudioMpsc(Arc<Mutex<mpsc::Receiver<f32>>>, u32);

impl AudioMpsc {
  pub fn new(receiver: mpsc::Receiver<f32>, sample_rate: u32) -> Self {
    Self(Mutex::new(receiver).into(), sample_rate)
  }
}

#[async_trait]
impl AudioSource for AudioMpsc {
  async fn next(&self) -> Option<f32> {
    let mut rx = self.0.lock().await;
    rx.next().await
  }

  fn sample_rate(&self) -> u32 {
    self.1
  }
}
