use serde::{Deserialize, Serialize};

pub const PACKET_MAX_SIZE: usize = 1430;

#[derive(Clone)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
  Connect { username: String },
  Ping,
  Voice { samples: Vec<i16> },
}

impl ClientMessage {
  pub fn to_bytes(&self) -> Vec<u8> {
    bincode::serialize(self).unwrap()
  }
  pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
    bincode::deserialize(bytes).ok()
  }
}

#[derive(Clone)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
  Pong,
  Voice { username: String, samples: Vec<i16> },
}

impl ServerMessage {
  pub fn to_bytes(&self) -> Vec<u8> {
    bincode::serialize(self).unwrap()
  }
  pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
    bincode::deserialize(bytes).ok()
  }
}