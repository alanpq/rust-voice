use std::{sync::{atomic::{AtomicBool, Ordering}, Mutex, Arc, RwLock, MutexGuard}, io::Write, ops::AddAssign};

use flexi_logger::{writers::LogWriter, DeferredNow, Record};
use log::{Log, info, error};
use pancurses::{initscr, noecho, endwin, Input, resize_term, COLOR_PAIR, has_colors, start_color, A_NORMAL, A_BOLD, half_delay};
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

const INVERT_OFFSET: u32 = 6;
const COLOR_TABLE: [i16; 6] = [
  pancurses::COLOR_WHITE,
  pancurses::COLOR_RED,
  pancurses::COLOR_YELLOW,
  pancurses::COLOR_GREEN,
  pancurses::COLOR_CYAN,
  pancurses::COLOR_BLUE,
];

impl App {
  pub fn new(pipe: LogPipe, client: Arc<Client>) -> Self {
    App {
      running: AtomicBool::new(false),
      pipe,
      client,
      server_addr: None,
    }
  }

  fn draw_logs(&self, window: &pancurses::Window, scroll: usize) -> usize {
    window.erase();
    window.attrset(A_NORMAL);
    let (max_y, max_x) = window.get_max_yx();
    window.mv(1, 1);
    let mut y = 1;
    let records = self.pipe.get();
    let start = records.len().saturating_sub((max_y as usize + scroll).saturating_sub(2));
    for i in start..records.len() {
      let log = &records[i];
      let mut x = 1;
      let level_color = match log.level {
        log::Level::Error => 1,
        log::Level::Warn => 2,
        log::Level::Info => 3,
        log::Level::Debug => 4,
        log::Level::Trace => 5,
      };
      window.attrset(COLOR_PAIR(level_color) | A_BOLD);
      let level = log.level.to_string();
      for c in level.chars() {
        window.mvaddch(y, (5_i32 - level.len() as i32) + x, c);
        x += 1;
        if x >= max_x -1 {
          x = 1;
          y += 1;
        }
      }
      x = 6;
      window.attrset(COLOR_PAIR(0) | A_NORMAL);
      let line = format!(": {}", log.body);
      for c in line.chars() {
        window.mvaddch(y, x, c);
        x += 1;
        if x >= max_x -1 {
          x = 1;
          y += 1;
        }
      }
      window.attrset(A_NORMAL);
      y += 1;
      if y >= max_y -1 {
        break;
      }
    }
    scroll.clamp(0, records.len().saturating_sub(max_y as usize))
  }

  fn draw_top_bar(&mut self, window: &pancurses::Window) {
    window.mv(0, 0);
    match &self.server_addr {
      Some(txt) => {
        window.attrset(COLOR_PAIR(3+INVERT_OFFSET) | A_BOLD);
        window.addstr(txt);
      }, 
      None => {
        if self.client.connected() {
          self.server_addr = Some(format!(" Connected to {}", self.client.server_addr()));
        }
        window.attrset(COLOR_PAIR(INVERT_OFFSET) | A_BOLD);
        window.addstr(" Not connected");
      }
    }
    let max_x = window.get_max_x();
    let cur_x = window.get_cur_x();
    for i in cur_x..max_x {
      window.mvaddch(0, i, ' ');
    }
  }

  pub fn stop(&self) {
    self.running.store(false, Ordering::SeqCst);
  }

  pub fn run(&mut self) {
    self.running.store(true, Ordering::SeqCst);
    let window = initscr();
    if has_colors() {
      start_color();
  }
    for (i, color) in COLOR_TABLE.iter().enumerate() {
      pancurses::init_pair(i as i16, *color, pancurses::COLOR_BLACK);
      pancurses::init_pair((i as i16) + INVERT_OFFSET as i16, pancurses::COLOR_BLACK, *color);
    }

    window.keypad(true);
    window.nodelay(true);
    half_delay(1);
    noecho();
    let mut log_scroll = 0;

    let (h, w) = window.get_max_yx();

    let log_window = window.subwin(h - 1, (w as f32 * 0.75) as i32, 1, 0).unwrap();

    while self.running.load(Ordering::SeqCst) {
      window.attrset(COLOR_PAIR(0));
      self.draw_top_bar(&window);
      log_scroll = self.draw_logs(&log_window, log_scroll);
      log_window.border('|', '|', '=', '=', '+', '+', '+', '+');
      // log_window.touch();
      match window.getch() {
        Some(Input::KeyMouse) => {
        }
        Some(Input::Character(x)) => {
          match x {
            'q' => self.stop(),
            _ => {}
          }
        },
        Some(Input::KeyResize) => {
          resize_term(0, 0);
        }
        Some(Input::KeyUp) => {
          log_scroll += 1;
        }
        Some(Input::KeyDown) => {
          log_scroll = log_scroll.saturating_sub(1);
        }
        // Some(input) => {
        //   error!("{:?}", input);
        // },
        _ => (),
      }
      window.touch();
      window.refresh();
      
    }
    endwin();
  }
}