use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

#[derive(Debug, Default)]
pub struct AtomicCounter(AtomicUsize);

impl AtomicCounter {
  pub fn inc(&self) -> usize {
    self.add(1)
  }

  pub fn add(&self, amount: usize) -> usize {
    self.0.fetch_add(amount, Relaxed)
  }

  pub fn get(&self) -> usize {
    self.0.load(Relaxed)
  }

  pub fn reset(&self) -> usize {
    self.0.swap(0, Relaxed)
  }

  pub fn into_inner(self) -> usize {
    self.0.into_inner()
  }
}
