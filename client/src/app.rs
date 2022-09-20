use std::{sync::{Arc, Mutex}, collections::HashMap, net::ToSocketAddrs};

use common::packets::ServerMessage;
use kira::manager::{AudioManager, AudioManagerSettings};
use log::{warn, info};
use ringbuf::{Producer, RingBuffer};
use uuid::Uuid;

use crate::{voice::{VoiceSoundHandle, VoiceSoundData, VoiceSoundSettings}, decoder::OpusDecoder, mic::MicService, client::Client, cpal::CpalBackend};

use anyhow::anyhow;

pub struct Peer {
  pub id: Uuid, 
}

type AMutex<T> = Arc<Mutex<T>>;
type ThreadMap<K,V> = AMutex<HashMap<K,V>>;

pub struct App {
  sound_map: ThreadMap<Uuid, VoiceSoundHandle>,
  producer_map: ThreadMap<Uuid, Producer<f32>>,
  decoder_map: ThreadMap<Uuid, OpusDecoder>,

  audio_manager: AMutex<AudioManager<CpalBackend>>,
  mic_service: MicService,
  client: Client,

  /// Sample rate of the playback device.
  sample_rate: u32,
}

impl App {

  pub fn new(username: String, latency_ms: f32) -> Result<Self, anyhow::Error> {

    let mut audio_manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())?;
    let sample_rate = audio_manager.backend_mut().sample_rate();

    let (mic_service, rx) = MicService::builder().with_latency(latency_ms).build()?;

    let mut client = Client::new(username, rx)?;

    Ok(Self {
      sound_map   : Arc::new(Mutex::new(HashMap::new())),
      producer_map: Arc::new(Mutex::new(HashMap::new())),
      decoder_map : Arc::new(Mutex::new(HashMap::new())),

      audio_manager: Arc::new(Mutex::new(audio_manager)),
      mic_service,
      client,

      sample_rate,
    })
  }

  pub fn start<A>(&mut self, addr: A) -> Result<(), anyhow::Error> where A: ToSocketAddrs {
    self.client.connect(addr)?;
    self.mic_service.start()?;
    Ok(())
  }

  pub fn poll(&mut self) -> Result<Option<ServerMessage>, anyhow::Error> {
    let msg = self.client.poll()?;
    match msg {
      Some(ref msg) => {
        match msg {
          ServerMessage::Voice{user, samples} => {
            self.handle_voice(*user, samples)?;
          },
          ServerMessage::Connected(user) => {
            info!("'{}' has joined.", user.username);
            self.create_peer(user.id)?;
          },
          ServerMessage::Pong => {},
        }
      },
      None => {}
    }
    Ok(msg)
  }

  fn create_peer(&self, id: Uuid) -> Result<(), anyhow::Error> {
    let latency = self.mic_service.latency();
    let mut sound_map = self.sound_map.lock().unwrap();
    if sound_map.contains_key(&id) {
      warn!("Peer already exists");
      return Ok(());
    }
    let (mut prod, cons) = RingBuffer::new(latency.samples() * 2).split();
    for _ in 0..latency.samples() {
      prod.push(0.0).unwrap();
    }
    let mut producer_map = self.producer_map.lock().unwrap();
    producer_map.insert(id, prod);

    let mut decoder_map = self.decoder_map.lock().unwrap();
    decoder_map.insert(id, OpusDecoder::new(self.sample_rate)?);

    let sound = VoiceSoundData::new(VoiceSoundSettings {
      ..Default::default()
    }, cons);

    let mut audio_manager = self.audio_manager.lock().unwrap();
    sound_map.insert(id, audio_manager.play(sound)?);

    Ok(())
  }

  fn handle_voice(&self, id: Uuid, data: &Vec<u8>) -> Result<(), anyhow::Error> {
    let mut decoder_map = self.decoder_map.lock().unwrap();
    let decoder = decoder_map.get_mut(&id).ok_or_else(|| anyhow!("No decoder for peer"))?;
    match decoder.decode(data) {
      Ok(data) => {
        let mut producer_map = self.producer_map.lock().unwrap();
        let producer = producer_map.get_mut(&id).ok_or_else(|| anyhow!("No producer for peer"))?;
        producer.push_slice(&data);
      },
      Err(e) => {
        warn!("Failed to decode voice data: {}", e);
      }
    }

    Ok(())
  }
}