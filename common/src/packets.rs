use serde::{Deserialize, Serialize};

use crate::{PeerID, UserInfo};

pub const PACKET_MAX_SIZE: usize = 32_768;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
  /// request to connect to a server
  Connect {
    username: String,
  },
  Disconnect,
  Ping,
  /// send voice to the server
  Voice {
    seq_num: SeqNum,
    samples: Vec<u8>,
  },
}

impl ClientMessage {
  pub fn to_bytes(&self) -> Result<Vec<u8>, Box<bincode::ErrorKind>> {
    bincode::serialize(self)
  }
  pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
    bincode::deserialize(bytes).ok()
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
  Pong,
  /// a user connected
  Connected(UserInfo),
  Disconnected(UserInfo),
  /// voice packet from a user
  Voice(AudioPacket<u8>),
}

impl ServerMessage {
  pub fn to_bytes(&self) -> Vec<u8> {
    bincode::serialize(self).unwrap()
  }
  pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
    bincode::deserialize(bytes).ok()
  }
}

use std::{cmp::Ordering, fmt::Display, ops};

#[repr(transparent)]
#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SeqNum(pub u16);

impl SeqNum {
  pub const MAX: Self = SeqNum(u16::MAX);
}

impl Display for SeqNum {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(f)
  }
}

impl From<u16> for SeqNum {
  fn from(value: u16) -> Self {
    Self(value)
  }
}

impl ops::Add for SeqNum {
  type Output = Self;

  fn add(self, rhs: Self) -> Self::Output {
    Self(self.0.wrapping_add(rhs.0))
  }
}
impl ops::Add<u16> for SeqNum {
  type Output = Self;

  fn add(self, rhs: u16) -> Self::Output {
    Self(self.0.wrapping_add(rhs))
  }
}
impl ops::AddAssign for SeqNum {
  fn add_assign(&mut self, rhs: Self) {
    self.0 += rhs.0
  }
}
impl ops::AddAssign<u16> for SeqNum {
  fn add_assign(&mut self, rhs: u16) {
    self.0 += rhs
  }
}

impl Eq for SeqNum {}

impl Ord for SeqNum {
  fn cmp(&self, other: &Self) -> Ordering {
    let (a, b) = (self.0, other.0);
    if a == b {
      return Ordering::Equal;
    }

    const HALF: u16 = u16::MAX / 2;
    if (a > b && a - b <= HALF) || (a < b && b - a > HALF) {
      return Ordering::Greater;
    }
    Ordering::Less
  }
}

impl PartialOrd for SeqNum {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

#[cfg(test)]
mod tests {
  use super::SeqNum;

  fn greater_than<S: Into<SeqNum>>(a: S, b: S, greater_than: bool) {
    let (a, b): (SeqNum, SeqNum) = (a.into(), b.into());
    assert_eq!(a > b, greater_than);
    assert_eq!(a < b, !greater_than);
  }

  #[test]
  fn test_sequence_ordering() {
    greater_than(0, 1, false);
    greater_than(0, u16::MAX, true);
    greater_than(32768, u16::MAX, false);
    greater_than(32767, u16::MAX, true);
    // TODO: add more just in case
  }
}

// FIXME: everything is f32 for now
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioPacket<T = f32> {
  pub seq_num: SeqNum,
  pub peer_id: PeerID,
  pub data: Vec<T>,
}

pub const FRAME_SIZE: usize = 1920;
#[derive(Copy, Clone, Debug)]
pub struct AudioFrame<T = f32>
where
  T: Copy,
{
  pub seq_num: SeqNum,
  pub data: [T; FRAME_SIZE],
}

impl<T: ops::AddAssign + Copy> ops::AddAssign for AudioFrame<T> {
  fn add_assign(&mut self, rhs: Self) {
    for i in 0..self.data.len() {
      self.data[i] += rhs.data[i];
    }
  }
}
impl PartialEq for AudioFrame {
  fn eq(&self, other: &Self) -> bool {
    self.seq_num == other.seq_num
  }
}

impl Eq for AudioFrame {}

impl Ord for AudioFrame {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.seq_num.cmp(&other.seq_num)
  }
}

impl PartialOrd for AudioFrame {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl TryFrom<AudioPacket> for AudioFrame {
  type Error = bool;

  fn try_from(value: AudioPacket) -> Result<Self, Self::Error> {
    Ok(AudioFrame {
      seq_num: value.seq_num,
      data: value.data[..PACKET_MAX_SIZE.min(value.data.len())]
        .try_into()
        .map_err(|_v: _| false)?,
    })
  }
}

impl PartialEq for AudioPacket {
  fn eq(&self, other: &Self) -> bool {
    self.seq_num == other.seq_num
  }
}

impl Eq for AudioPacket {}

impl Ord for AudioPacket {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.seq_num.cmp(&other.seq_num)
  }
}

impl PartialOrd for AudioPacket {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}
