use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use ringbuf::RingBuffer;

mod packets;

fn main() -> Result<(), anyhow::Error> {
  let host = cpal::default_host();
  let output_device = host.default_output_device().expect("no output device available");
  let input_device = host.default_input_device().expect("no input device available");

  println!("Output device: {:?}", output_device.name()?);
  println!("Input device: {:?}", input_device.name()?);

  let config: cpal::StreamConfig = input_device.default_input_config()?.into();
  println!("Default input config: {:?}", config);

  let latency = 140.0;
  let latency_frames = (latency / 1000.) * config.sample_rate.0 as f32;
  let latency_samples = latency_frames as usize * config.channels as usize;

  let buffer = RingBuffer::new(latency_samples * 2);
  let (mut producer, mut consumer) = buffer.split();

  // fill buffer with silence for latency delay
  for _ in 0..latency_samples {
    producer.push(0.0).unwrap(); // ring buffer has 2x latency, so unwrap will never fail
  }

  let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
    let mut output_fell_behind = false;
    for &sample in data {
      if producer.push(sample).is_err() {
        output_fell_behind = true;
      }
    }
    if output_fell_behind {
      eprintln!("output stream fell behind: try increasing latency");
    }
  };

  let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    let mut input_fell_behind = false;
    for sample in data {
      *sample = match consumer.pop() {
        Some(s) => s,
        None => {
          input_fell_behind = true;
          0.0
        }
      };
    }
    if input_fell_behind {
      eprintln!("input stream fell behind: try increasing latency");
    }
  };

  let input_stream = input_device.build_input_stream(&config, input_data_fn, err_fn)?;
  let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn)?;

  input_stream.play()?;
  output_stream.play()?;

  println!("Press enter to stop");
  let mut stdin = std::io::stdin();
  stdin.read_line(&mut String::new())?;
  drop(input_stream);
  drop(output_stream);

  Ok(())
}

fn err_fn(err: cpal::StreamError) {
  eprintln!("an error occurred on stream: {}", err);
}