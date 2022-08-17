use serde::{Deserialize, Serialize};

pub const PACKET_MAX_SIZE: usize = 1430;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
  Connect { username: String },
  Ping,
  Voice { samples: Vec<i16> },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
  Pong,
  Voice { username: String, samples: Vec<i16> },
}