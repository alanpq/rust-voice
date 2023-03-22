use std::time::Duration;

use cpal::{traits::{DeviceTrait, StreamTrait, HostTrait}, StreamConfig};

use log::{error, info, debug};
use ringbuf::{HeapConsumer, HeapProducer};
use anyhow::{anyhow, bail};

use crate::Latency;

pub fn get_config(device: &cpal::Device) -> anyhow::Result<cpal::StreamConfig> {
  let supported_configs = device.supported_output_configs()?;
  let config = {
    let mut out = None;
    for config in supported_configs {
      if out.is_some() { break; }
      for rate in crate::opus::OPUS_SAMPLE_RATES {
        if config.channels() == 2 && config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate && config.sample_format() == cpal::SampleFormat::F32 {
          out = Some(config.with_sample_rate(cpal::SampleRate(rate)));
          break;
        }
      }
    }
    out
  }.ok_or_else(|| anyhow!("could not get supported output config!"))?;
  if config.sample_format() != cpal::SampleFormat::F32 {
    bail!("sample format not supported: {}", config.sample_format())
  }
  Ok(config.into())
}

fn error_fn(err: cpal::StreamError) {
  error!("{}", err);
}
pub fn make_stream(
  device: &cpal::Device,
  config: &cpal::StreamConfig,
  consumer: HeapConsumer<f32>
) -> Result<cpal::Stream, cpal::BuildStreamError> {
  let mut consumer = consumer.into_postponed();
  let channels = config.channels as usize;
  let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    // debug!("{}/{} = {}", data.len(), channels, data.len()/channels);
    for i in 0..data.len()/channels {
      // currently input is mono, so we copy data for each channel
      let sample = consumer.pop().unwrap_or(0.0);
      for j in 0..channels {
        data[i*channels + j] = sample;
      }
    }
    consumer.sync(); // postpone sync to avoid sync on every individual sample pop
  };
  device.build_output_stream(config, data_fn, error_fn, None)
}
