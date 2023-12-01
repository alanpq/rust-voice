mod async_drop;
mod client;
mod conn;
mod log_pipe;

extern crate client as lib;

use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::Arc;

use conn::Connection;
use flexi_logger::{Logger, LoggerHandle, WriteMode};
use iced::widget::{button, column, row, scrollable, text, text_input, Column};
use iced::{
  executor, font, Alignment, Application, Command, Element, Font, Length, Sandbox, Settings,
  Subscription, Theme,
};
use log::{debug, info};
use log_pipe::LogPipe;

use once_cell::sync::Lazy;

static SCROLLABLE_ID: Lazy<scrollable::Id> = Lazy::new(scrollable::Id::unique);

const FONT: Font = Font::with_name("Cabin");
pub fn main() -> anyhow::Result<()> {
  let pipe = LogPipe::new();
  let logger = Logger::try_with_str("app=debug,client=debug")?
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

struct App {
  log: LogPipe,
  logger: LoggerHandle,
  inner: Inner,

  address: String,
  username: String,

  connection: Option<Connection>,
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
        logger: flags.logger.unwrap(),
        inner: Inner::default(),
        connection: None,

        address: std::env::var("ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".to_string()),
        username: std::env::var("USER").unwrap_or_default(),
      },
      font::load(include_bytes!("../fonts/Cabin-Regular.ttf").as_slice()).map(Message::FontLoaded),
    )
  }

  fn title(&self) -> String {
    String::from("rust-voice")
  }

  fn subscription(&self) -> Subscription<Self::Message> {
    conn::client().map(Message::Client)
  }

  fn update(&mut self, message: Message) -> Command<Message> {
    debug!("{message:?}");
    if let Message::Client(conn::Event::Ready(c)) = &message {
      self.connection = Some(c.clone());
    }
    match &mut self.inner {
      Inner::Home {} => match message {
        Message::Connect => {
          if let Some(ref mut conn) = &mut self.connection {
            info!("Connecting...");
            conn.try_send(conn::Input::Connect(self.username.clone(), self.address.parse().unwrap())).unwrap(/* FIXME: remove this */);
            self.inner = Inner::Connecting;
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
      Inner::Connecting => {
        if let Message::Client(conn::Event::Connected) = message {
          self.inner = Inner::Connected {}
        }
      }
      Inner::Connected {} => match message {
        Message::Client(c) => match c {
          conn::Event::Ready(_) => {}
          conn::Event::Connected => {
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
      Inner::Connecting => text("Connecting...").into(),
      Inner::Connected {} => {
        let logs = Column::with_children(
          self
            .log
            .get()
            .iter()
            .map(|m| {
              row![
                text(format!("{}", m.level)).width(Length::Fixed(60.0)),
                text(format!("{}", m.body)),
              ]
              .into()
            })
            .collect(),
        );
        let logs = scrollable(logs).id(SCROLLABLE_ID.clone());
        let btn = button("Disconnect").on_press(Message::Disconnect);
        column![btn, logs].into()
      }
    }
  }
}
