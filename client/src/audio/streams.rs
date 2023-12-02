use std::sync::Arc;

use cpal::traits::DeviceTrait as _;
use futures::executor::block_on;
use log::error;

use super::Statistics;

pub(super) fn error(err: cpal::StreamError) {
  error!("{}", err);
}

pub(super) fn make_input_stream(
  device: cpal::Device,
  config: cpal::StreamConfig,
  mut mic_tx: futures::channel::mpsc::Sender<f32>,
  stats: Arc<Statistics>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
  let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
    for sample in data.iter().step_by(config.channels as usize) {
      if mic_tx.try_send(*sample).is_err() {
        stats.dropped_mic_samples.inc();
      }
    }
  };
  device.build_input_stream(&config, data_fn, error, None)
}

pub(super) fn make_output_stream(
  device: cpal::Device,
  config: cpal::StreamConfig,
  sources: super::AudioSources,
  stats: Arc<Statistics>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
  let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    {
      let channels = config.channels as usize;
      for i in 0..data.len() / channels {
        let sample = block_on(async {
          let mut sample = 0.0;

          // TODO: this probably sucks, either make this an async mutex or kill yourself idk
          let sources: Vec<_> = {
            let sources = sources.lock().unwrap();
            sources.iter().cloned().collect()
          };
          for s in sources.iter() {
            if let Some(s) = s.next().await {
              sample += s;
            }
          }
          sample
        });
        // since currently all input is mono, we must duplicate the sample for every channel
        for j in 0..channels {
          data[i * channels + j] = sample;
        }
      }
      stats.pushed_output_samples.add(data.len());
    }
  };
  device.build_output_stream(&config, data_fn, error, None)
}