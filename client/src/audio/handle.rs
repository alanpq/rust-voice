use std::sync::{mpsc, Arc};

use crate::{source::AudioSource, Latency};

use super::{AudioServiceBuilder, AudioSources, Message};

pub struct AudioHandle {
  pub(super) sources: AudioSources,

  pub(super) in_latency: Latency,
  pub(super) out_latency: Latency,

  pub(super) in_config: cpal::StreamConfig,
  pub(super) out_config: cpal::StreamConfig,

  pub(super) tx: mpsc::Sender<Message>,
}

impl AudioHandle {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn play(&self) {
    self.tx.send(Message::Play);
  }

  pub fn pause(&self) {
    self.tx.send(Message::Pause);
  }

  pub fn stop(&self) {
    self.tx.send(Message::Stop);
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
