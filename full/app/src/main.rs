mod async_drop;
mod client;
mod conn;
mod log_pipe;

extern crate client as lib;

use anyhow::Context;
use common::Average;
use conn::Connection;
use dns_lookup::lookup_host;
use flexi_logger::{Logger, LoggerHandle, WriteMode};
use iced::widget::{button, column, row, scrollable, text, text_input, Column};
use iced::{
  executor, font, subscription, Application, Command, Element, Font, Length, Settings,
  Subscription, Theme,
};
use lib::audio::Statistics;
use log::{debug, error, info};
use log_pipe::LogPipe;
use std::net::SocketAddr;
use std::sync::Arc;

use once_cell::sync::Lazy;

static SCROLLABLE_ID: Lazy<scrollable::Id> = Lazy::new(scrollable::Id::unique);

const FONT: Font = Font::with_name("Cabin");
const FONT_MONO: Font = Font {
  family: font::Family::Name("Martian Mono"),
  monospaced: true,
  stretch: font::Stretch::Normal,
  weight: font::Weight::Normal,
};
pub fn main() -> anyhow::Result<()> {
  let pipe = LogPipe::new();
  let logger = Logger::try_with_env_or_str("app=debug,client=debug")?
    .log_to_writer(Box::new(pipe.clone()))
    .duplicate_to_stdout(flexi_logger::Duplicate::All)
    .write_mode(WriteMode::Async)
    .start()?;

  App::run(Settings {
    default_font: FONT,
    flags: Flags {
      log: pipe,
      logger: Some(logger),
    },
    ..Default::default()
  })?;
  Ok(())
}

enum Inner {
  Home {},
  Connecting,
  Connected {},
}
impl Default for Inner {
  fn default() -> Self {
    Self::Home {}
  }
}

#[derive(Default)]
struct App {
  log: LogPipe,
  #[allow(dead_code)]
  logger: Option<LoggerHandle>,
  inner: Inner,

  address: String,
  username: String,

  connection: Option<Connection>,

  last_clock: Option<std::time::Instant>,
  client_stats: Option<Arc<client::Statistics>>,
  last_client_stats: Option<client::Statistics<usize>>,
  out_bytes_avg: Average<32, f32>,
  in_bytes_avg: Average<32, f32>,
  out_pak_avg: Average<32, f32>,
  in_pak_avg: Average<32, f32>,

  audio_stats: Option<Arc<Statistics>>,
  last_pushed_samples: Option<(usize, std::time::Instant)>,
  out_samples_per_sec: Average<32, f32>,
}

#[derive(Default)]
struct Flags {
  log: LogPipe,
  logger: Option<LoggerHandle>,
}

#[derive(Debug, Clone)]
enum Message {
  Connect,
  Disconnect,
  SetAddress(String),
  SetUsername(String),
  FontLoaded(Result<(), font::Error>),
  Client(conn::Event),
  Clock(std::time::Instant),
}

impl Application for App {
  type Message = Message;
  type Flags = Flags;
  type Executor = executor::Default;
  type Theme = Theme;

  fn new(flags: Self::Flags) -> (Self, Command<Message>) {
    (
      Self {
        log: flags.log,
        logger: flags.logger,
        inner: Inner::default(),
        connection: None,
        client_stats: None,
        audio_stats: None,
        last_pushed_samples: None,

        address: std::env::var("ADDRESS").unwrap_or_else(|_| "gs.alanp.me".to_string()),
        username: std::env::var("USER").unwrap_or_default(),
        ..Default::default()
      },
      Command::batch(vec![
        font::load(include_bytes!("../fonts/Cabin-Regular.ttf").as_slice()),
        font::load(include_bytes!("../fonts/MartianMono-Regular.ttf").as_slice()),
      ])
      .map(Message::FontLoaded),
    )
  }

  fn title(&self) -> String {
    String::from("rust-voice")
  }

  fn subscription(&self) -> Subscription<Self::Message> {
    Subscription::batch(vec![
      conn::client().map(Message::Client),
      iced::time::every(std::time::Duration::from_millis(100)).map(Message::Clock),
    ])
  }

  fn update(&mut self, message: Message) -> Command<Message> {
    // debug!("{message:?}");
    match &message {
      Message::Client(conn::Event::Ready(c)) => {
        self.connection = Some(c.clone());
      }
      Message::Client(conn::Event::AudioStart(stats)) => {
        self.audio_stats = Some(stats.clone());
      }
      Message::Clock(now) => {
        if let Some(last_clock) = self.last_clock.replace(*now) {
          if let Some(stats) = &self.audio_stats {
            let now = (stats.pushed_output_samples(), *now);
            if let Some(then) = self.last_pushed_samples.replace(now) {
              self
                .out_samples_per_sec
                .push((now.0 - then.0) as f32 / (now.1.duration_since(then.1).as_secs_f32()));
            }
          }
          if let Some(stats) = &self.client_stats {
            let stats = stats.get();
            if let Some(last) = self.last_client_stats.replace(stats) {
              let secs = now.duration_since(last_clock).as_secs_f32();
              self
                .in_pak_avg
                .push((stats.packets_received - last.packets_received) as f32 / secs);
              self
                .out_pak_avg
                .push((stats.packets_sent - last.packets_sent) as f32 / secs);
              self
                .in_bytes_avg
                .push((stats.bytes_received - last.bytes_received) as f32 / secs);
              self
                .out_bytes_avg
                .push((stats.bytes_sent - last.bytes_sent) as f32 / secs);
            }
          }
        }
      }
      _ => {}
    }
    match &mut self.inner {
      Inner::Home {} => match message {
        Message::Connect => {
          if let Some(ref mut conn) = &mut self.connection {
            info!("Connecting...");
            match self
              .address
              .parse::<SocketAddr>()
              .context("could not parse socket addr")
              .or_else(|e| {
                debug!("{e} - trying dns resolution...");
                let split = self.address.splitn(2, ':').collect::<Vec<_>>();
                let host = split.first().context("could not get first in split")?;
                let port: u16 = split.get(1).and_then(|p| p.parse().ok()).unwrap_or(1234); // TODO: change default port
                let lookup = lookup_host(host).context("could not lookup host")?;
                let ip = lookup.first().context("no resolved ips")?;
                Ok::<_, anyhow::Error>(SocketAddr::new(*ip, port))
              }) {
              Ok(addr) => {
                debug!("Connecting to '{addr}'...");
                conn.try_send(conn::Input::Connect(self.username.clone(), addr)).unwrap(/* FIXME: remove this */);
                self.inner = Inner::Connecting;
              }
              Err(e) => {
                error!("Invalid address: {e}");
              }
            }
          }
        }
        Message::SetAddress(addr) => {
          self.address = addr;
        }
        Message::SetUsername(usr) => {
          self.username = usr;
        }
        Message::FontLoaded(_) => {}
        _ => {}
      },
      Inner::Connecting => match message {
        Message::Client(conn::Event::Connected(stats)) => {
          self.client_stats = Some(stats);
          self.inner = Inner::Connected {};
        }
        Message::Disconnect => {
          self.inner = Inner::default();
          self.client_stats = None;
        }
        _ => (),
      },
      Inner::Connected {} => match message {
        Message::Client(c) => match c {
          conn::Event::Connected(stats) => {
            self.client_stats = Some(stats);
            info!("Connected!");
            return scrollable::snap_to(SCROLLABLE_ID.clone(), scrollable::RelativeOffset::END);
          }
          conn::Event::Joined(user) => {
            info!("{} has joined the room.", user.username);
            return scrollable::snap_to(SCROLLABLE_ID.clone(), scrollable::RelativeOffset::END);
          }
          conn::Event::Left(user) => {
            info!("{} has left.", user.username);
            return scrollable::snap_to(SCROLLABLE_ID.clone(), scrollable::RelativeOffset::END);
          }
          _ => {}
        },
        Message::Disconnect => {
          info!("Disconnecting...");
          if let Some(ref mut conn) = self.connection {
            debug!("sending dc");
            let _ = conn.try_send(conn::Input::Disconnect);
          }
          self.inner = Inner::default();
        }
        _ => {}
      },
    }
    Command::none()
  }

  fn view(&self) -> Element<Message> {
    match &self.inner {
      Inner::Home {} => {
        let username = text_input("Username", &self.username).on_input(Message::SetUsername);
        let conn_widget = row![
          text_input("Address", &self.address).on_input(Message::SetAddress),
          button("Connect").on_press(Message::Connect)
        ];
        column![username, conn_widget].padding(20).into()
      }
      Inner::Connecting => column![
        text("Connecting..."),
        button("Cancel").on_press(Message::Disconnect)
      ]
      .into(),
      Inner::Connected {} => {
        let logs = Column::with_children(
          self
            .log
            .get()
            .iter()
            .map(|m| {
              text(format!("{:>5}: {}", m.level, m.body))
                .font(FONT_MONO)
                .size(12)
                .into()
            })
            .collect(),
        )
        .width(Length::Fill);
        let logs = scrollable(logs).id(SCROLLABLE_ID.clone());
        let btn = button("Disconnect").on_press(Message::Disconnect);

        let sidebar = match self.audio_stats.as_ref().zip(self.client_stats.as_ref()) {
          Some((audio, client)) => scrollable(column![
            text("Input:").font(FONT_MONO),
            text(format!(
              "  Dropped samples: {}",
              audio.dropped_mic_samples()
            ))
            .font(FONT_MONO),
            text("Output:").font(FONT_MONO),
            text(format!(
              "  Samples/sec: {:.3}",
              self.out_samples_per_sec.avg::<f32>()
            ))
            .font(FONT_MONO),
            text("Client:").font(FONT_MONO),
            text(format!(
              "   In bytes/sec: {:.3}",
              self.in_bytes_avg.avg::<f32>()
            ))
            .font(FONT_MONO),
            text(format!(
              "  Out bytes/sec: {:.3}",
              self.out_bytes_avg.avg::<f32>()
            ))
            .font(FONT_MONO),
            text(format!(
              "   In packets/sec: {:.3}",
              self.in_pak_avg.avg::<f32>()
            ))
            .font(FONT_MONO),
            text(format!(
              "  Out packets/sec: {:.3}",
              self.out_pak_avg.avg::<f32>()
            ))
            .font(FONT_MONO),
          ]),
          None => scrollable(column![text("no audio thread running")]),
        };
        column![
          btn,
          row![
            logs.width(Length::FillPortion(2)),
            sidebar.width(Length::FillPortion(1))
          ]
        ]
        .into()
      }
    }
  }
}
