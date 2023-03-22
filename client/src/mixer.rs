use std::{sync::{atomic::{AtomicUsize, Ordering, AtomicU16}, Mutex, Arc}, collections::VecDeque};

use common::{MAX_PEERS, PeerID, packets::{AudioFrame, FRAME_SIZE}};
use log::{warn, error, debug};

use common::{packets::{SeqNum, AudioPacket}};
use ringbuf::{HeapProducer, HeapConsumer, HeapRb};

type Peer<T> = (SeqNum, HeapProducer<T>, HeapConsumer<T>);


pub struct Mixer {
  to_speaker: HeapProducer<f32>,

  peers: [Option<VecDeque<AudioFrame>>; MAX_PEERS],
  active: Vec<usize>,
}

impl Mixer {
  const INIT: Option<VecDeque<AudioFrame>> = None;
  pub fn new(to_speaker: HeapProducer<f32>) -> Self {
    Self {
      to_speaker,

      peers: [Self::INIT; MAX_PEERS],
      active: Vec::new(),
    }
  }

  fn flush(&mut self) {
    for peer in self.peers.iter_mut().flatten() {
      peer.make_contiguous().sort();
    }

    loop {
      let mut frame = AudioFrame{seq_num: u16::MAX.into(), data: [0.0; FRAME_SIZE]};
      let mut did_something = false;
      for i in &self.active {
        let peer = self.peers.get_mut(*i).unwrap().as_mut().unwrap();
        if let Some(f) = peer.pop_front() {
          frame += f;
          did_something = true;
        }
      }
      if self.to_speaker.push_slice(&frame.data) != frame.data.len() || !did_something {
        break;
      }
    }
  }

  pub fn tick(&mut self) {

  }

  pub fn push_data(&mut self, data: AudioPacket) {
    let peer = self.peers.get_mut(data.peer_id as usize).expect("peer id out of bounds");
    match peer {
      Some(peer) => {
        let len = data.data.len();
        if let Ok(data) = data.try_into() {
          peer.push_back(data);
        } else {
          error!("bad pak len: {len}");
        }
        if peer.len() > 5 { // TODO: use latency here
          self.flush();
        }
      },
      None => warn!("Could not find peer {}", data.peer_id),
    }
  }

  pub fn add_peer(&mut self, id: PeerID) {
    let peer = self.peers.get_mut(id as usize).expect("peer id out of bounds");
    match peer {
      Some(_) => warn!("trying to create existing peer {id}"),
      None => {
        let _ = peer.insert(VecDeque::new());
        self.active.push(id as usize);
      },
    }
  }
}

// TODO: do i care enough about making this generic across numeric