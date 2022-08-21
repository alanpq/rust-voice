use std::sync::{Mutex, Arc};

pub struct OpusDecoder {
  decoder: Arc<Mutex<opus::Decoder>>,
  frame_size: usize,
}

impl OpusDecoder {
  pub fn new(sample_rate: u32) -> Result<Self, anyhow::Error> {
    let decoder = opus::Decoder::new(sample_rate, opus::Channels::Mono)?;
    let frame_size = (sample_rate as usize * 20) / 1000;
    Ok(Self {
      decoder: Arc::new(Mutex::new(decoder)),
      frame_size,
    })
  }

  pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<f32>, anyhow::Error> {
    let mut decoder = self.decoder.lock().unwrap();
    let mut output = vec![0.0; self.frame_size];
    decoder.decode_float(&packet, &mut output, true)?;
    Ok(output)
  }
}