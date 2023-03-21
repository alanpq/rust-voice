use std::{sync::{atomic::{AtomicUsize, Ordering}, Mutex, Arc}, collections::VecDeque};

use log::{warn, error};

use crate::{packet::{SeqNum, AudioPacket}, PeerID, MAX_PEERS};

type Peer<T> = (SeqNum, VecDeque<T>);

pub fn mixer() -> (Arc<MixerHandle>, Mixer) {
  let handle = Arc::new(MixerHandle {

  });

  let mixer = Mixer::new(handle.clone());

  (handle, mixer)
}

pub struct MixerHandle {

}

// TODO: do i care enough about making this generic across numeric
pub struct Mixer {
  handle: Arc<MixerHandle>,
  peers: [Option<Mutex<Peer<f32>>>; MAX_PEERS],
}

impl Mixer {
  const INIT: Option<Mutex<Peer<f32>>> = None;

  pub(crate) fn new(handle: Arc<MixerHandle>) -> Self {
    Self {
      handle,
      peers: [Self::INIT; MAX_PEERS],
    }
  }

  pub fn pop_frame(&self) -> f32 {
    let mut acc = 0.0;
    for peer in self.peers.iter().flatten() {
      let mut peer = peer.lock().unwrap();
      acc += peer.1.pop_front().unwrap_or_default();
    }
    acc
  }

  pub fn add_peer(&mut self, id: PeerID) {
    let peer = self.peers.get_mut(id as usize).expect("peer id out of bounds");
    match peer {
      Some(_) => warn!("trying to add existing peer {id}!"),
      None => {
        let _ = peer.insert(Mutex::new((SeqNum::MAX, VecDeque::new())));
      }
    }
  }

  pub fn push_data(&self, mut packet: AudioPacket) {
    let peer = self.peers.get(packet.peer_id as usize).expect("peer id out of bounds");
    match peer {
      Some(p) => {
        let mut p = p.lock().unwrap();
        if p.0 >= packet.seq_num {return;}
        p.1.extend(packet.data.drain(..));
      },
      None => error!("peer {} not found for packet", packet.peer_id),
    }
  }
}