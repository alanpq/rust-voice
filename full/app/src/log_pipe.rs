use flexi_logger::{writers::LogWriter, DeferredNow};
use log::Record;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone)]
pub struct LogRecord {
  pub level: log::Level,
  pub body: String,
}

pub struct LogPipe {
  producer: Arc<Mutex<HeapProducer<LogRecord>>>,
  consumer: Arc<Mutex<HeapConsumer<LogRecord>>>,
  records: Arc<Mutex<Vec<LogRecord>>>,
}

impl Clone for LogPipe {
  fn clone(&self) -> Self {
    Self {
      producer: self.producer.clone(),
      consumer: self.consumer.clone(),
      records: self.records.clone(),
    }
  }
}

impl LogPipe {
  pub fn new() -> Self {
    let buf = HeapRb::new(2048);
    let (producer, consumer) = buf.split();
    Self {
      records: Arc::new(Mutex::new(Vec::new())),
      producer: Arc::new(Mutex::new(producer)),
      consumer: Arc::new(Mutex::new(consumer)),
    }
  }

  pub fn get(&self) -> MutexGuard<Vec<LogRecord>> {
    let mut records = self.records.lock().unwrap();
    let mut consumer = self.consumer.lock().unwrap();

    while let Some(record) = consumer.pop() {
      records.push(record);
    }

    drop(consumer);
    records
  }
}
impl Default for LogPipe {
    fn default() -> Self {
        Self::new()
    }
}

impl LogWriter for LogPipe {
  fn write(&self, now: &mut DeferredNow, record: &Record<'_>) -> std::io::Result<()> {
    let mut producer = self.producer.lock().unwrap();
    producer
      .push(LogRecord {
        level: record.level(),
        body: record.args().to_string(),
      })
      .expect("could not push record");
    Ok(())
  }

  fn flush(&self) -> std::io::Result<()> {
    Ok(())
  }
}
