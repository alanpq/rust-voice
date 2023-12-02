use std::{iter::Sum, ops::Div};

use num_traits::AsPrimitive;

pub struct Average<const N: usize, S: Default + Copy> {
  idx: usize,
  max_idx: usize,
  samples: [S; N],
}

impl<const N: usize, S> Average<N, S>
where
  S: Default + Copy,
{
  pub fn new() -> Self {
    Self {
      idx: 0,
      max_idx: 0,
      samples: [S::default(); N],
    }
  }
  pub fn push(&mut self, sample: S) {
    self.samples[self.idx] = sample;
    self.idx = (self.idx + 1) % N;
    self.max_idx = (self.max_idx + 1).min(N);
  }
}

impl<const N: usize, S: Default + Copy> Average<N, S> {
  pub fn avg<O>(&self) -> O
  where
    O: Div<O, Output = O> + Sum<O> + Copy + 'static,
    S: AsPrimitive<O>,
    usize: AsPrimitive<O>,
  {
    let total: O = self.samples[..self.max_idx].iter().map(|s| s.as_()).sum();
    let size: O = (N.min(self.max_idx)).as_();
    total / size
  }
}

impl<const N: usize, S: Default + Copy + Sum<S>> Default for Average<N, S> {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn avg_i32_f32_unfinished() {
    let mut avg: Average<5, i32> = Average::new();
    avg.push(-10);
    avg.push(21);
    assert_eq!(avg.avg::<f32>(), 5.5);
  }

  #[test]
  fn avg_f64_i32() {
    let mut avg: Average<5, f64> = Average::new();
    avg.push(99999999.0);
    avg.push(100.2);
    avg.push(10.66);
    avg.push(45.1203);
    avg.push(78.999);
    avg.push(101.0);
    assert_eq!(avg.avg::<i32>(), 66);
  }

  #[test]
  fn avg_usize_f32() {
    let mut avg: Average<5, usize> = Average::new();
    avg.push(99999999);
    avg.push(100);
    avg.push(10);
    avg.push(45);
    avg.push(78);
    avg.push(101);
    assert_eq!(avg.avg::<f32>(), 66.8);
  }

  #[test]
  fn avg_usize_f64() {
    let mut avg: Average<5, usize> = Average::new();
    avg.push(99999999);
    avg.push(100);
    avg.push(10);
    avg.push(45);
    avg.push(78);
    avg.push(101);
    assert_eq!(avg.avg::<f64>(), 66.8);
  }

  #[test]
  fn avg_f64_f64() {
    let mut avg: Average<5, f64> = Average::new();
    avg.push(99999999.0);
    avg.push(100.2);
    avg.push(10.66);
    avg.push(45.1203);
    avg.push(78.999);
    avg.push(101.0);
    assert_eq!(avg.avg::<f64>(), 67.19586);
  }
}
