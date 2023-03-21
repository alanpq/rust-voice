use std::{cmp::Ordering};

use crate::PeerID;

#[repr(transparent)]
#[derive(PartialEq, Clone, Copy)]
pub struct SeqNum(u16);

impl SeqNum {
  pub const MAX: Self = SeqNum(u16::MAX);
}

impl From<u16> for SeqNum {
  fn from(value: u16) -> Self {
    Self(value)
  }
}

impl Eq for SeqNum {}

impl Ord for SeqNum {
  fn cmp(&self, other: &Self) -> Ordering {
    let (a, b) = (self.0, other.0);
    if a == b { return Ordering::Equal; }
    
    const HALF: u16 = u16::MAX/2;
    if (a > b && a - b <= HALF )
      || (a < b && b - a > HALF) {
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
  use std::cmp::Ordering;
  use super::SeqNum;

  fn greater_than<S: Into<SeqNum>>(a: S, b: S, greater_than: bool) {
    let (a,b): (SeqNum, SeqNum) = (a.into(), b.into());
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
pub struct AudioPacket<T = f32> { 
  pub seq_num: SeqNum,
  pub peer_id: PeerID,
  pub data: Vec<T>,
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
