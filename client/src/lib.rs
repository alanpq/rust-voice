mod latency;
pub use latency::*;
use ringbuf::HeapRb;

pub mod playback;
pub mod microphone;
pub mod packet;

pub mod mixer;

mod opus;

pub mod client;

pub fn audio_thread() -> std::thread::JoinHandle<()> {
  std::thread::spawn(move || {
    // let mut playback = PlaybackService::builder()
    // .build().unwrap();
    // let mut producer = playback.start().unwrap();
    
  })
}

pub fn make_buffer(latency: Latency) -> HeapRb<f32> {
  let mut buf = ringbuf::HeapRb::new(latency.samples() * 2);
  for _ in 0..latency.samples() {
    ringbuf::Rb::push(&mut buf, 0.0).unwrap(/* buffer has 2x latency */);
  }
  buf
}