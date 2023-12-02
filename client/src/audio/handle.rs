use std::sync::{mpsc, Arc};

use log::error;

use crate::{source::AudioSource, Latency};

use super::{AudioServiceBuilder, AudioSources, Message, Statistics};

pub struct AudioHandle {
  pub(super) sources: AudioSources,

  pub(super) in_latency: Latency,
  pub(super) out_latency: Latency,

  pub(super) in_config: cpal::StreamConfig,
  pub(super) out_config: cpal::StreamConfig,

  pub(super) tx: mpsc::Sender<Message>,

  pub stats: Arc<Statistics>,
}

impl AudioHandle {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn play(&self) {
    if let Err(e) = self.tx.send(Message::Play) {
      error!("could not play - {e:?}")
    }
  }

  pub fn pause(&self) {
    if let Err(e) = self.tx.send(Message::Pause) {
      error!("could not pause - {e:?}")
    }
  }

  pub fn stop(&self) {
    if let Err(e) = self.tx.send(Message::Stop) {
      error!("could not stop - {e:?}")
    }
  }

  pub fn add_source(&self, source: Arc<dyn AudioSource>) {
    self.sources.lock().unwrap().push(source)
  }

  pub fn in_cfg(&self) -> &cpal::StreamConfig {
    &self.in_config
  }
  pub fn out_cfg(&self) -> &cpal::StreamConfig {
    &self.out_config
  }

  pub fn in_latency(&self) -> Latency {
    self.in_latency
  }
  pub fn out_latency(&self) -> Latency {
    self.out_latency
  }
}

impl Drop for AudioHandle {
  fn drop(&mut self) {
    self.stop();
  }
}
