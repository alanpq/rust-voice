use std::sync::Arc;

use kira::{Volume, sound::{Sound, SoundData}, dsp::Frame, track::TrackId, tween::Tweener};
use ringbuf::Consumer;

pub struct VoiceSoundSettings {
  pub volume: Volume,
  pub track: TrackId,
  pub pitch: f64,
}

impl Default for VoiceSoundSettings {
  fn default() -> Self {
    Self { volume: Volume::Amplitude(1.0), track: TrackId::Main, pitch: 1.0 }
  }
}

pub struct VoiceSoundData {
  pub settings: VoiceSoundSettings,
  pub consumer: Consumer<f32>,
}

impl VoiceSoundData {
  pub fn new(settings: VoiceSoundSettings, consumer: Consumer<f32>) -> Self {
    Self { settings, consumer }
  }

  pub(crate) fn split(self) -> Result<(VoiceSound, VoiceSoundHandle), anyhow::Error> {
    let sound = VoiceSound {
      volume: Tweener::new(self.settings.volume),
      pitch: self.settings.pitch,
      consumer: self.consumer,
      shared: Arc::new(Shared {
      }),
      time: 0.0,
    };
    let handle = VoiceSoundHandle {};
    Ok((sound, handle))
  }
}

impl SoundData for VoiceSoundData {
  type Error = anyhow::Error;
  type Handle = VoiceSoundHandle;
  
  fn into_sound(self) -> Result<(Box<dyn Sound>, Self::Handle), Self::Error> {
      let (sound, handle) = self.split()?;
      Ok((Box::new(sound), handle))
  }
}

pub struct VoiceSoundHandle {

}


pub(crate) struct Shared {

}

pub(crate) struct VoiceSound {
  time: f64,
  volume: Tweener<Volume>,
  shared: Arc<Shared>,
  pitch: f64,
  consumer: Consumer<f32>,
}

impl Sound for VoiceSound {
  fn track(&mut self) -> kira::track::TrackId {
    kira::track::TrackId::Main
  }

  fn process(&mut self, dt: f64, clock_info_provider: &kira::clock::clock_info::ClockInfoProvider) -> kira::dsp::Frame {
    self.time += dt;
    if let Some(sample) = self.consumer.pop() {
      Frame::from_mono(sample)
    } else {
      Frame::from_mono(0.0)
    }
  }

  fn finished(&self) -> bool {
    false
  }
}