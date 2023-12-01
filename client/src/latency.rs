#[derive(Copy, Clone)]
pub struct Latency {
  pub ms: f32,
  pub frames: usize,
  pub samples: usize,
}

impl Latency {
  pub fn new(latency_ms: f32, sample_rate: u32, channels: u16) -> Self {
    let frames = ((latency_ms * sample_rate as f32) / 1000.0) as usize;
    let samples = frames * channels as usize;

    Self {
      ms: latency_ms,
      frames,
      samples,
    }
  }

  pub fn samples(&self) -> usize {
    self.samples
  }
}
