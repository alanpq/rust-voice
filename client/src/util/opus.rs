pub const OPUS_SAMPLE_RATES: [u32; 5] = [
  48000,
  24000,
  16000,
  12000,
  8000,
];

pub fn nearest_opus_rate(sample_rate: u32) -> Option<u32> {
  OPUS_SAMPLE_RATES.iter().min_by_key(|rate| rate.abs_diff(sample_rate)).copied()
}