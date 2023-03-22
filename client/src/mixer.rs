use std::{sync::{atomic::{AtomicUsize, Ordering, AtomicU16}, Mutex, Arc}, collections::VecDeque};

use common::{MAX_PEERS, PeerID, packets::{AudioFrame, FRAME_SIZE}};
use log::{warn, error, debug};

use common::{packets::{SeqNum, AudioPacket}};
use ringbuf::{HeapProducer, HeapConsumer, HeapRb};

type Peer<T> = (SeqNum, HeapProducer<T>, HeapConsumer<T>);


pub struct Mixer {
  to_speaker: HeapProducer<f32>,

  peers: [Option<Channel>; MAX_PEERS],
  active: Vec<usize>,
  max_frames: usize,
}

struct Channel {
  buffer: VecDeque<AudioFrame>,
}

impl Channel {
  pub fn new() -> Self {
    Self {buffer: VecDeque::new()}
  }
  pub fn next(&mut self) -> Option<AudioFrame> {
    self.buffer.make_contiguous().sort();
    self.buffer.pop_front()
  }
  pub fn push(&mut self, frame: AudioFrame) {
    self.buffer.push_back(frame);
  }
}

impl Mixer {
  const INIT: Option<Channel> = None;
  pub fn new(to_speaker: HeapProducer<f32>) -> Self {
    Self {
      to_speaker,

      peers: [Self::INIT; MAX_PEERS],
      active: Vec::new(),
      max_frames: 0,
    }
  }

  // fn flush(&mut self) {
  //   for peer in self.peers.iter_mut().flatten() {
  //     peer.make_contiguous().sort();
  //   }

  //   for i in 0..self.max_frames {
  //     let mut frame = AudioFrame{seq_num: u16::MAX.into(), data: [0.0; FRAME_SIZE]};
  //     let mut did_something = false;
  //     for i in &self.active {
  //       let peer = self.peers.get_mut(*i).unwrap().as_mut().unwrap();
  //       if let Some(f) = peer.pop_front() {
  //         frame += f;
  //         did_something = true;
  //       }
  //     }
  //     if self.to_speaker.push_slice(&frame.data) != frame.data.len() || !did_something {
  //       self.max_frames -= i;
  //       break;
  //     }
  //   }
  //   self.max_frames = 0;
  // }

  pub fn tick(&mut self) -> bool {
    let mut frame = AudioFrame {data: [0.0; FRAME_SIZE], seq_num: SeqNum::MAX};
    let mut did_push = false;
    for peer in &self.active {
      let peer = self.peers[*peer].as_mut().unwrap();
      if let Some(f) = peer.next() {
        frame += f;
        did_push = true;
      }
    }
    if !did_push { return false; }
    debug!("tick {}", frame.data.len());
    if self.to_speaker.push_slice(&frame.data) != frame.data.len() {
      warn!("mixer going too hard!!!");
    }
    did_push
  }

  pub fn push_data(&mut self, data: AudioPacket) {
    let peer = self.peers.get_mut(data.peer_id as usize).expect("peer id out of bounds");
    match peer {
      Some(peer) => {
        let len = data.data.len();
        if let Ok(data) = data.try_into() {
          peer.push(data);
        } else {
          error!("bad pak len: {len}");
        }
        // if peer.len() > 10 { // TODO: use latency here
          // self.flush();
        // }
      },
      None => warn!("Could not find peer {}", data.peer_id),
    }
  }

  pub fn add_peer(&mut self, id: PeerID) {
    let peer = self.peers.get_mut(id as usize).expect("peer id out of bounds");
    match peer {
      Some(_) => warn!("trying to create existing peer {id}"),
      None => {
        let _ = peer.insert(Channel::new());
        self.active.push(id as usize);
      },
    }
  }
}

// TODO: do i care enough about making this generic across numeric