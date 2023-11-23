use std::{collections::HashMap, sync::{Mutex, Arc, RwLock, mpsc::Sender, atomic::{AtomicUsize, Ordering}}, time::{Instant, Duration}};

use log::warn;
use ringbuf::{Producer, Consumer, HeapRb, HeapConsumer, HeapProducer};

use crate::{
    latency::Latency,
    source::AudioSource,
};
use core::pin::Pin;
use futures::{
    stream::Stream,
    sink::Sink,
    task::{Context, Poll},
};
use super::OpusDecoder;

const EXPECTED_PEERS: usize = 4;

struct Channel<S = f32> {
    pub producer: Mutex<HeapProducer<S>>,
    pub consumer: Mutex<HeapConsumer<S>>,
}
impl<S: Default + Copy + std::fmt::Debug> Channel<S> {
    pub fn new(latency: &Latency) -> Self {
        let buf = HeapRb::new(latency.samples()*2);
        let (mut producer, consumer) = buf.split();
        for _ in 0..latency.samples() {
          producer.push(Default::default()).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
        }
        Self { producer: producer.into(), consumer: consumer.into() }
    }
    pub fn pop(&self) -> Option<S> {
        self.consumer.lock().unwrap().pop()
    }
    pub fn push_slice(&self, samples: &[S]) -> usize {
        self.producer.lock().unwrap().push_slice(samples)
    }
}

// service to mix peer audio together
pub struct PeerMixer {
  sample_rate: u32,
  latency: Latency,

  peers: AtomicUsize,

  channels: Arc<RwLock<HashMap<u32, Channel>>>,
  decoder_map: Arc<RwLock<HashMap<u32, OpusDecoder>>>,
}

impl PeerMixer {
  pub fn new(sample_rate: u32, latency: Latency) -> Self {
    Self {
      sample_rate,
      latency,
      peers: AtomicUsize::new(0),
      channels: Arc::new(RwLock::new(HashMap::with_capacity(EXPECTED_PEERS))),
      decoder_map: Arc::new(RwLock::new(HashMap::with_capacity(EXPECTED_PEERS))),
    }
  }

  pub fn push(&self, peer: u32, packet: &[u8]) {
    let mut decoders = self.decoder_map.write().unwrap();
    if !decoders.contains_key(&peer) {
      drop(decoders);
      self.add_peer(peer);
      decoders = self.decoder_map.write().unwrap();
      warn!("Lazy adding decoder for peer {}", peer);
    }
    let decoder = decoders.get_mut(&peer).expect("decoder not found");
    match decoder.decode(packet) {
      Ok(output) => {
        let channels = self.channels.read().unwrap();
        let channel = channels.get(&peer).expect("producer not found");
        channel.push_slice(&output);
      }
      Err(e) => {
        warn!("could not decode packet: {}", e);
      }
    }
  }

  pub fn add_peer(&self, id: u32) {
    let mut decoder_map = self.decoder_map.write().unwrap();
    if decoder_map.contains_key(&id) {
      warn!("peer {} already exists", id);
      decoder_map.get_mut(&id).unwrap().reset();
      return;
    }

    let decoder = OpusDecoder::new(self.sample_rate).unwrap();

    self.channels.write().unwrap().insert(id, Channel::new(&self.latency));

    decoder_map.insert(id, decoder);
    self.peers.fetch_add(1, Ordering::SeqCst);
  }

  // TODO: can probably pool these
  // decoders can have their state reset
  pub fn remove_peer(&self, id: u32) {
    let mut channels = self.channels.write().unwrap();
    let mut decoder_map = self.decoder_map.write().unwrap();
    if !channels.contains_key(&id) {
      warn!("peer {} does not exist", id);
      return;
    }
    channels.remove(&id);
    decoder_map.remove(&id);
  }
}

impl AudioSource for PeerMixer {
    fn next(&self) -> Option<f32> {
        let channels = self.channels.read().unwrap();
        let mut sample: Option<f32> = None;
        for (_, channel) in channels.iter() {
            if let Some(s) = channel.pop() {
                sample = Some(sample.unwrap_or_default() + s);
            }
        }
        sample
    }
}
