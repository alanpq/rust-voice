use log::debug;

pub fn resample_audio(source: &[f32], source_rate: u32, dest_rate: u32) -> Vec<f32> {
  let dst_size = (source.len() as f32 * (dest_rate as f32 / source_rate as f32)) as usize;
  let last_pos = source.len() - 1;
  let mut dst = vec![0.0; dst_size];
  for i in 0..dst_size {
    let pos = ((i as u32 * source_rate) as f32 / dest_rate as f32);
    let p1 = pos as usize;
    let coef = pos - (p1 as f32);
    let p2 = if p1 == last_pos { last_pos } else { p1 + 1 };
    dst[i] = (1. - coef) * source[p1] + coef * source[p2];
  }
  // debug!("Resampled {} samples -> {} samples from {} hz -> {} hz", source.len(), dst_size, source_rate, dest_rate);
  dst
}