use std::{io::stdin, net::{UdpSocket, SocketAddr}, collections::HashMap, sync::{Mutex, Arc}};

use clap::Parser;
use env_logger::Env;

use log::{info, error};
use ringbuf::{Producer, RingBuffer};
use tracing::{span, Level};

mod voice;
mod mic;
mod util;
mod latency;
mod client;
mod cpal;
mod decoder;

#[derive(Parser, Debug)]
#[clap(name="Rust Voice Server")]
struct Args {
  #[clap(value_parser)]
  address: String,
  #[clap(value_parser = clap::value_parser!(u16).range(1..), short='p', long="port", default_value_t=8080)]
  port: u16,
  #[clap(value_parser, long="latency", default_value_t=150.)]
  latency: f32,
}

use kira::{manager::{
  AudioManager, AudioManagerSettings,
}, sound::Sound, dsp::Frame, Volume};
use uuid::Uuid;
use voice::{VoiceSoundData, VoiceSoundSettings};

use crate::{client::Client, mic::MicService, voice::VoiceSoundHandle, cpal::CpalBackend, decoder::OpusDecoder};
pub struct Peer {
  pub id: Uuid,
  
}

fn main() -> Result<(), anyhow::Error> {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
  let args = Args::parse();
  
  let mut manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())?;
  let sound_map: Arc<Mutex<HashMap<Uuid, VoiceSoundHandle>>> = Arc::new(Mutex::new(HashMap::new()));
  let producer_map: Arc<Mutex<HashMap<Uuid, Producer<f32>>>> = Arc::new(Mutex::new(HashMap::new()));
  let decoder_map: Arc<Mutex<HashMap<Uuid, OpusDecoder>>> = Arc::new(Mutex::new(HashMap::new()));

  let (mut mic_service, rx) = MicService::builder().with_latency(100.).build()?;

  let latency = mic_service.latency();

  let mut client = Client::new("test".to_owned(), rx)?;
  
  let addr: SocketAddr = format!("{}:{}", args.address, args.port).parse()?;
  client.connect(addr)?;

  {
    // let sound_map = sound_map.clone();
    // let producer_map = producer_map.clone();
    client.on_voice(Box::new(move |id, data| {
      let mut sound_map = sound_map.lock().unwrap();
      let e = sound_map.entry(id).or_insert_with(|| {
        let ring = RingBuffer::new(latency.samples()*2);
        let (mut producer, consumer) = ring.split();
        for _ in 0..latency.samples() {
          producer.push(0.0).unwrap();
        }
        let mut producer_map = producer_map.lock().unwrap();
        producer_map.insert(id, producer);
        let mut decoder_map = decoder_map.lock().unwrap();
        decoder_map.insert(id, OpusDecoder::new(48000).unwrap());
        let sound = VoiceSoundData::new(VoiceSoundSettings {volume: Volume::Amplitude(0.5), ..Default::default()}, consumer);
        manager.play(sound).unwrap()
      });
      let mut decoder_map = decoder_map.lock().unwrap();
      if let Ok(data) = decoder_map.get_mut(&id).unwrap().decode(&data) {
        let mut producer_map = producer_map.lock().unwrap();
        producer_map.get_mut(&id).unwrap().push_slice(&data);
      }
    }));
  }

  mic_service.start()?;

  client.run()?;

  // let mut voice = VoiceSoundData { settings: VoiceSoundSettings::default() };
  // voice.settings.volume = Volume::Amplitude(0.4);
  // manager.play(voice)?;
  
  println!("Press enter to exit.");
  stdin().read_line(&mut "".into())?;
  
  Ok(())
}