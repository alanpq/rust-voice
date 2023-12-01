use std::sync::mpsc;

use cpal::{traits::StreamTrait as _, Stream};

pub enum Message {
  Play,
  Pause,
  Stop,
}

pub struct AudioService {
  pub(super) input_stream: Stream,
  pub(super) output_stream: Stream,

  pub(super) rx: mpsc::Receiver<Message>,
}

impl AudioService {
  pub fn run(&self) {
    while let Ok(m) = self.rx.recv() {
      match m {
        Message::Play => {
          self.input_stream.play();
        }
        Message::Pause => {
          self.input_stream.pause();
        }
        Message::Stop => {
          return;
        }
      }
    }
  }
}
