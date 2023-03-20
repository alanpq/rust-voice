use anyhow::anyhow;
use cpal::traits::DeviceTrait;
use log::{error, debug, warn};
use ringbuf::{HeapProducer};

pub fn get_config(device: &cpal::Device) -> anyhow::Result<cpal::StreamConfig> {

  let supported_configs = device.supported_input_configs()?;

  let config: cpal::StreamConfig = {
    let mut out = None;
    for config in supported_configs {
      if out.is_some() { break; }
      for rate in crate::opus::OPUS_SAMPLE_RATES {
        if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
          out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
          break;
        }
      }
    }
    out
  }.ok_or_else(|| anyhow!("could not get supported input config!"))?;

  Ok(config)
}

fn error_fn(err: cpal::StreamError) {
  error!("{}", err);
}

pub fn make_stream(device: &cpal::Device, config: &cpal::StreamConfig, producer: HeapProducer<f32>) -> Result<cpal::Stream, cpal::BuildStreamError> {
  let mut producer = producer.into_postponed();
  let channels = config.channels as usize;
  let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
    // get only 1st channel from mic input
    // TODO: optional stereo input support?
    // debug!("{}", data.len());
    for sample in data.iter().step_by(channels) {
      if producer.push(*sample).is_err() {
        warn!("cant keep up!");
      }
    }
    producer.sync(); // postpone sync to avoid sync on every individual sample push
  };
  device.build_input_stream(config, data_fn, error_fn, None)
}