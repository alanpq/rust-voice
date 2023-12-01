use anyhow::{anyhow, Context as _};
use cpal::{
  traits::{DeviceTrait, HostTrait, StreamTrait},
  BuildStreamError, Stream, StreamConfig,
};
use futures::executor::block_on;
use log::{debug, error, info, warn};
use std::sync::{
  mpsc::{self, Receiver, Sender},
  Arc, Mutex,
};

use crate::{
  latency::Latency,
  source::{AudioMpsc, AudioSource},
  util::opus::OPUS_SAMPLE_RATES,
};

type AudioSources = Arc<Mutex<Vec<Arc<dyn AudioSource>>>>;

enum Message {
  Play,
  Pause,
  Stop,
}

struct AudioService {
  input_stream: Stream,
  output_stream: Stream,

  rx: Receiver<Message>,
}

impl AudioService {
  pub fn run(&self) {
    while let Ok(m) = self.rx.recv() {
      match m {
        Message::Play => {
          self.input_stream.play();
        }
        Message::Pause => {
          self.input_stream.pause();
        }
        Message::Stop => {
          return;
        }
      }
    }
  }
}

pub struct AudioHandle {
  sources: AudioSources,

  in_latency: Latency,
  out_latency: Latency,

  in_config: cpal::StreamConfig,
  out_config: cpal::StreamConfig,

  tx: Sender<Message>,
}

impl AudioHandle {
  pub fn builder() -> AudioServiceBuilder {
    AudioServiceBuilder::new()
  }

  pub fn play(&self) {
    self.tx.send(Message::Play);
  }

  pub fn pause(&self) {
    self.tx.send(Message::Pause);
  }

  pub fn stop(&self) {
    self.tx.send(Message::Stop);
  }

  pub fn add_source(&self, source: Arc<dyn AudioSource>) {
    self.sources.lock().unwrap().push(source)
  }

  pub fn in_cfg(&self) -> &cpal::StreamConfig {
    &self.in_config
  }
  pub fn out_cfg(&self) -> &cpal::StreamConfig {
    &self.out_config
  }

  pub fn in_latency(&self) -> Latency {
    self.in_latency
  }
  pub fn out_latency(&self) -> Latency {
    self.out_latency
  }
}

impl Drop for AudioHandle {
  fn drop(&mut self) {
    self.stop();
  }
}

fn error(err: cpal::StreamError) {
  error!("{}", err);
}

fn make_input_stream(
  device: cpal::Device,
  config: StreamConfig,
  mut mic_tx: futures::channel::mpsc::Sender<f32>,
) -> Result<Stream, BuildStreamError> {
  let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
    for sample in data.iter().step_by(config.channels as usize) {
      if let Err(e) = mic_tx.try_send(*sample) {
        // warn!("failed to send mic data to mic_tx: {:?}", e);
      }
    }
  };
  device.build_input_stream(&config, data_fn, error, None)
}

fn make_output_stream(
  device: cpal::Device,
  config: StreamConfig,
  sources: AudioSources,
) -> Result<Stream, BuildStreamError> {
  let data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    {
      let channels = config.channels as usize;
      for i in 0..data.len() / channels {
        let sample = block_on(async {
          let mut sample = 0.0;
          let sources = sources.lock().unwrap();
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
    }
  };
  device.build_output_stream(&config, data_fn, error, None)
}

pub struct AudioServiceBuilder {
  host: cpal::Host,
  output_device: Option<cpal::Device>,
  input_device: Option<cpal::Device>,
  latency_ms: f32,
  sources: Vec<Arc<dyn AudioSource>>,
}

impl AudioServiceBuilder {
  pub fn new() -> Self {
    Self {
      host: cpal::default_host(),
      output_device: None,
      input_device: None,
      latency_ms: 150.0,
      sources: Vec::new(),
    }
  }

  pub fn with_source(mut self, source: Arc<dyn AudioSource>) -> Self {
    self.sources.push(source);
    self
  }

  pub fn with_latency(mut self, latency_ms: f32) -> Self {
    self.latency_ms = latency_ms;
    self
  }

  pub fn start(self) -> Result<(AudioHandle, AudioMpsc), anyhow::Error> {
    let output_device = self.output_device.unwrap_or(
      self
        .host
        .default_output_device()
        .ok_or_else(|| anyhow!("no output device available"))?,
    );
    let input_device = self.input_device.unwrap_or(
      self
        .host
        .default_input_device()
        .ok_or_else(|| anyhow!("no input device available"))?,
    );
    info!("Output device: {:?}", output_device.name()?);
    info!("Input device: {:?}", input_device.name()?);

    let in_config: cpal::StreamConfig = match input_device.supported_input_configs() {
      Result::Ok(configs) => {
        let mut out = None;
        for config in configs {
          if out.is_some() {
            break;
          }
          for rate in OPUS_SAMPLE_RATES {
            if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
              out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
              break;
            }
          }
        }
        out
      }
      Err(_) => None,
    }
    .unwrap_or_else(|| {
      input_device
        .default_input_config()
        .expect("could not get default input config")
        .into()
    });

    debug!("Default input config: {:?}", in_config);

    let out_config: cpal::StreamConfig = match output_device.supported_output_configs() {
      Result::Ok(configs) => {
        let mut out = None;
        for config in configs {
          if out.is_some() {
            break;
          }
          for rate in OPUS_SAMPLE_RATES {
            if config.max_sample_rate().0 >= rate && config.min_sample_rate().0 <= rate {
              out = Some(config.with_sample_rate(cpal::SampleRate(rate)).into());
              break;
            }
          }
        }
        out
      }
      Err(_) => None,
    }
    .unwrap_or_else(|| {
      output_device
        .default_output_config()
        .expect("could not get default output config")
        .into()
    });
    debug!("Default output config: {:?}", out_config);

    info!("Input:");
    info!(" - Channels: {}", in_config.channels);
    info!(" - Sample Rate: {}", in_config.sample_rate.0);

    info!("Output:");
    info!(" - Channels: {}", out_config.channels);
    info!(" - Sample Rate: {}", out_config.sample_rate.0);

    let out_latency = Latency::new(
      self.latency_ms,
      out_config.sample_rate.0,
      out_config.channels,
    );
    let in_latency = Latency::new(
      self.latency_ms,
      out_config.sample_rate.0,
      out_config.channels,
    );

    let (mic_tx, mic_rx) = futures::channel::mpsc::channel(4096);

    let sources = Arc::new(Mutex::new(self.sources));

    let mic = AudioMpsc::new(mic_rx, in_config.sample_rate.0);

    let (tx, rx) = mpsc::channel();

    {
      let sources = sources.clone();
      let in_config = in_config.clone();
      let out_config = out_config.clone();
      std::thread::spawn(move || {
        let input_stream = make_input_stream(input_device, in_config, mic_tx).unwrap();
        let output_stream = make_output_stream(output_device, out_config, sources).unwrap();
        let service = AudioService {
          input_stream,
          output_stream,
          rx,
        };
        service.run();
      });
    }

    Ok((
      AudioHandle {
        sources,
        out_latency,
        out_config,
        in_latency,
        in_config,
        tx,
      },
      mic,
    ))
  }
}

impl Default for AudioServiceBuilder {
  fn default() -> Self {
    Self::new()
  }
}
