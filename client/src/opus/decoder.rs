use std::sync::{Mutex, Arc};
use log::{info, warn};

use super::nearest_opus_rate;

pub struct OpusDecoder {
  /// the real sample rate of the input
  sample_rate: u32,
  /// the sample rate of the encoder
  opus_rate: u32,
  
  decoder: Arc<Mutex<opus::Decoder>>,
  frame_size: usize,
}

impl OpusDecoder {
  pub fn new(sample_rate: u32) -> Result<Self, anyhow::Error> {
    let opus_rate = nearest_opus_rate(sample_rate).unwrap();
    let frame_size = (opus_rate * 40) as usize / 1000;
    // (48000 * 2.5 * 10) / 1000
    info!("Creating new OpusDecoder with frame size {} @ opus:{} hz (real:{} hz)", frame_size, opus_rate, sample_rate);
    
    if opus_rate != sample_rate {
      warn!("Audio Resampling is not yet supported! Your audio will likely be distorted/pitched.");
    }

    let decoder = opus::Decoder::new(opus_rate, opus::Channels::Mono)?;
    Ok(Self {
      opus_rate,
      sample_rate,
      decoder: Arc::new(Mutex::new(decoder)),
      frame_size,
    })
  }

  pub fn frame_size(&self) -> usize {
    self.frame_size
  }

  pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<f32>, anyhow::Error> {
    let mut decoder = self.decoder.lock().unwrap();
    let mut output = vec![0.0; self.frame_size];
    decoder.decode_float(packet, &mut output[..], false)?;
    Ok(output)
  }

  pub fn reset(&self) {
    let mut decoder = self.decoder.lock().unwrap();
    decoder.reset_state();
  }
}