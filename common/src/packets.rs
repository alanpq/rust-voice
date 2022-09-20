use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::UserInfo;

pub const PACKET_MAX_SIZE: usize = 4000;

#[derive(Clone)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
  /// request to connect to a server
  Connect { username: String },
  Ping,
  /// send voice to the server
  Voice { samples: Vec<f32> },
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
  /// a user connected
  Connected (UserInfo),
  /// voice packet from a user
  Voice { user: Uuid, samples: Vec<f32> },
}

impl ServerMessage {
  pub fn to_bytes(&self) -> Vec<u8> {
    bincode::serialize(self).unwrap()
  }
  pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
    bincode::deserialize(bytes).ok()
  }
}