use std::{collections::HashMap, sync::{Mutex, Arc, mpsc::Sender}, time::{Instant, Duration}};

use log::warn;
use ringbuf::{Consumer, Producer, RingBuffer};

use crate::latency::Latency;

use super::OpusDecoder;

const EXPECTED_PEERS: usize = 4;

// service to mix peer audio together
pub struct PeerMixer {
  sample_rate: u32,
  latency: Latency,

  producer_map: Arc<Mutex<HashMap<u32, Producer<f32>>>>,
  consumer_map: Arc<Mutex<HashMap<u32, Consumer<f32>>>>,
  decoder_map: Arc<Mutex<HashMap<u32, OpusDecoder>>>,

  output_tx: Mutex<Sender<f32>>,

  last_push: Mutex<Instant>,
  push_rate: Duration,
}

impl PeerMixer {
  pub fn new(sample_rate: u32, latency: Latency, output_tx: Sender<f32>) -> Self {
    Self {
      sample_rate,
      latency,
      producer_map: Arc::new(Mutex::new(HashMap::with_capacity(EXPECTED_PEERS))),
      consumer_map: Arc::new(Mutex::new(HashMap::with_capacity(EXPECTED_PEERS))),
      decoder_map: Arc::new(Mutex::new(HashMap::with_capacity(EXPECTED_PEERS))),
      output_tx: Mutex::new(output_tx),
      last_push: Mutex::new(Instant::now()),
      push_rate: Duration::from_secs_f32(1.0/(sample_rate as f32)),
    }
  }

  pub fn push(&self, peer: u32, packet: &[u8]) {
    let mut decoders = self.decoder_map.lock().unwrap();
    // FIXME: remove this hack
    if !decoders.contains_key(&peer) {
      warn!("HACK: adding decoder for peer {}", peer);
      drop(decoders);
      self.add_peer(peer);
      decoders = self.decoder_map.lock().unwrap();
    }
    let decoder = decoders.get_mut(&peer).expect("decoder not found");
    match decoder.decode(packet) {
      Ok(output) => {
        let mut producers = self.producer_map.lock().unwrap();
        let producer = producers.get_mut(&peer).expect("producer not found");
        producer.push_slice(&output);
      }
      Err(e) => {
        warn!("could not decode packet: {}", e);
      }
    }
  }

  pub fn tick(&self) {
    // let last_push = self.last_push.lock().unwrap();
    // if self.last_push.elapsed() > self.push_rate {

    // }
    let mut consumers = self.consumer_map.lock().unwrap();
    let mut sample = 0.0;
    let mut real_data = false;
    for (_peer, consumer) in consumers.iter_mut() {
      if let Some(s) = consumer.pop() {
        sample += s;
        real_data = true;
      }
    }
    if real_data {
      let output_tx = self.output_tx.lock().unwrap();
      if let Err(e) = output_tx.send(sample) {
        warn!("could not push sample: {}", e);
      }
    }
  }

  pub fn add_peer(&self, id: u32) {
    let mut decoder_map = self.decoder_map.lock().unwrap();
    if decoder_map.contains_key(&id) {
      warn!("peer {} already exists", id);
      return;
    }

    let decoder = OpusDecoder::new(self.sample_rate).unwrap();

    let buf = RingBuffer::new(self.latency.samples()*2);
    let (mut producer, consumer) = buf.split();
    for _ in 0..self.latency.samples() {
      producer.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
    }

    self.producer_map.lock().unwrap().insert(id, producer);
    self.consumer_map.lock().unwrap().insert(id, consumer);
    decoder_map.insert(id, decoder);
  }

  // TODO: can probably pool these
  // decoders can have their state reset
  pub fn remove_peer(&self, id: u32) {
    let mut producer_map = self.producer_map.lock().unwrap();
    let mut consumer_map = self.consumer_map.lock().unwrap();
    let mut decoder_map = self.decoder_map.lock().unwrap();
    if !producer_map.contains_key(&id) {
      warn!("peer {} does not exist", id);
      return;
    }
    producer_map.remove(&id);
    consumer_map.remove(&id);
    decoder_map.remove(&id);
  }
}