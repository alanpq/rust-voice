use std::{sync::{atomic::{AtomicBool, Ordering}, Mutex, Arc, RwLock, MutexGuard}, io::{Write, self}, ops::AddAssign, time::Duration};

use crossterm::{terminal, QueueableCommand, cursor, style::{self, Stylize as _, Color}, event::{read, poll, Event, KeyEvent, KeyCode}, ExecutableCommand as _};
use flexi_logger::{writers::LogWriter, DeferredNow, Record};
use log::{Log, info, error};
use ringbuf::{Producer, Consumer};

use crate::client::Client;

#[derive(Debug)]
#[derive(Clone)]
pub struct LogRecord {
  pub level: log::Level,
  pub body: String,
}

pub struct LogPipe {
  producer: Arc<Mutex<Producer<LogRecord>>>,
  consumer: Arc<Mutex<Consumer<LogRecord>>>,
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
    let buf = ringbuf::RingBuffer::new(2048);
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

impl LogWriter for LogPipe {
  fn write(&self, now: &mut DeferredNow, record: &Record<'_>) -> std::io::Result<()> {
    let mut producer = self.producer.lock().unwrap();
    producer.push(LogRecord {
      level: record.level(),
      body: record.args().to_string(),
    }).expect("could not push record");
    Ok(())
  }

  fn flush(&self) -> std::io::Result<()> {
    Ok(())
  }
}

pub struct App {
  running: AtomicBool,
  pipe: LogPipe,
  client: Arc<Client>,
  server_addr: Option<String>,
}

impl App {
  pub fn new(pipe: LogPipe, client: Arc<Client>) -> Self {
    App {
      running: AtomicBool::new(false),
      pipe,
      client,
      server_addr: None,
    }
  }

  fn draw_logs(&self, stdout: &mut std::io::Stdout, window: &Window, scroll: usize) -> anyhow::Result<usize> {
    stdout.queue(terminal::Clear(terminal::ClearType::All))?;
    let (max_x, max_y) = window.get_max_yx();
    stdout.queue(cursor::MoveTo(window.x + 1, window.y + 1))?;
    let mut y = 1;
    let records = self.pipe.get();
    let start = records.len().saturating_sub((max_y as usize + scroll).saturating_sub(2));
    for i in start..records.len() {
      let log = &records[i];
      let mut x = 1;
      let level_color = match log.level {
        log::Level::Error => (Color::Red),
        log::Level::Warn => (Color::Yellow),
        log::Level::Info => (Color::Green),
        log::Level::Debug => (Color::Blue),
        log::Level::Trace => (Color::Cyan),
      };
      //window.attrset(COLOR_PAIR(level_color) | A_BOLD);
      let level = log.level.to_string();
      for c in level.chars() {
        stdout.queue(cursor::MoveTo((5 - level.len()) as u16 + x, y))?;
        stdout.queue(style::PrintStyledContent(c.bold().with(level_color)))?;
        x += 1;
        if x >= max_x -1 {
          x = 1;
          y += 1;
        }
      }
      x = 6;
      // window.attrset(COLOR_PAIR(0) | A_NORMAL);
      let line = format!(": {}", log.body);
      for c in line.chars() {
        stdout.queue(cursor::MoveTo(x, y))?;
        stdout.queue(style::Print(c))?;
        x += 1;
        if x >= max_x -1 {
          x = 1;
          y += 1;
        }
      }
      // window.attrset(A_NORMAL);
      y += 1;
      if y >= max_y -1 {
        break;
      }
    }
    Ok(scroll.clamp(0, records.len().saturating_sub(max_y as usize)))
  }

  fn draw_top_bar(&mut self, stdout: &mut std::io::Stdout) -> anyhow::Result<()> {
    stdout.queue(cursor::MoveTo(0,0))?;
    match &self.server_addr {
      Some(txt) => {
        // window.attrset(COLOR_PAIR(3+INVERT_OFFSET) | A_BOLD);
        stdout.queue(style::PrintStyledContent(txt.clone().bold().negative()))?;
      }, 
      None => {
        if self.client.connected() {
          self.server_addr = Some(format!(" Connected to {}", self.client.server_addr()));
        }
        stdout.queue(style::PrintStyledContent(" Not connected".bold().negative()))?;//(COLOR_PAIR(INVERT_OFFSET) | A_BOLD);
      }
    }
    // let max_x = window.get_max_x();
    // let cur_x = window.get_cur_x();
    // for i in cur_x..max_x {
    //   window.mvaddch(0, i, ' ');
    // }
    Ok(())
  }

  pub fn stop(&self) {
    self.running.store(false, Ordering::SeqCst);
  }

  pub fn run(&mut self) -> anyhow::Result<()> {
    self.running.store(true, Ordering::SeqCst);
    let mut log_scroll: usize = 0;
    let mut stdout = io::stdout();

    let (w, h) = terminal::size().unwrap(); // TODO: remove unwrap
    let log_window = Window {
        x: 0,
        y: 1,
        w: w -1,
        h: h -1,
    };

    while self.running.load(Ordering::SeqCst) {
      log_scroll = self.draw_logs(&mut stdout, &log_window, log_scroll)?;
      self.draw_top_bar(&mut stdout)?;
      // TODO: log_window.border('|', '|', '=', '=', '+', '+', '+', '+');
      // log_window.touch();
      stdout.flush()?;
      if poll(Duration::from_millis(100))? {
          match read()? {
            Event::Mouse(_) => {
            }
            Event::Key(x) => {
              match x.code  {
                KeyCode::Char('q') => self.stop(),
                KeyCode::Up => {
                    log_scroll += 1;
                }
                KeyCode::Down => {
                    log_scroll = log_scroll.saturating_sub(1);
                }
                _ => {}
              }
            },
            Event::Resize(w,h) => {
            
            }
            // Some(input) => {
            //   error!("{:?}", input);
            // },
            _ => (),
          }
    }
      
    }
    Ok(())
  }
}

pub struct Window {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}
impl Window {
    pub fn get_max_yx(&self) -> (u16, u16) {
        (self.w + self.x, self.h + self.y)
    }
}
