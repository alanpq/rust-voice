use common::AtomicCounter;

#[derive(Default, Debug)]
pub struct Statistics {
  pub(crate) dropped_mic_samples: AtomicCounter,

  pub(crate) pushed_output_samples: AtomicCounter,
}

impl Statistics {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn dropped_mic_samples(&self) -> usize {
    self.dropped_mic_samples.get()
  }

  pub fn pushed_output_samples(&self) -> usize {
    self.pushed_output_samples.get()
  }
}
