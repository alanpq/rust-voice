mod conn;
mod log_pipe;

extern crate client as lib;

use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::Arc;

use conn::Connection;
use flexi_logger::{Logger, LoggerHandle, WriteMode};
use iced::widget::{button, column, row, text, text_input};
use iced::{
  executor, font, Alignment, Application, Command, Element, Font, Sandbox, Settings, Subscription,
  Theme,
};
use log::info;
use log_pipe::LogPipe;

const FONT: Font = Font::with_name("Cabin");
pub fn main() -> anyhow::Result<()> {
  let pipe = LogPipe::new();
  // let logger = Logger::try_with_env_or_str("app=debug")?
  let logger = Logger::try_with_env_or_str("app,client")?
    .log_to_stdout()
    .add_writer("Box", Box::new(pipe.clone()))
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
  Home { address: String, username: String },
  Connecting,
  Connected {},
}
impl Default for Inner {
  fn default() -> Self {
    Self::Home {
      address: "127.0.0.1:8080".to_string(),
      username: "user".to_string(),
    }
  }
}

struct App {
  log: LogPipe,
  logger: LoggerHandle,
  inner: Inner,

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
    if let Message::Client(conn::Event::Ready(c)) = &message {
      self.connection = Some(c.clone());
    }
    match &mut self.inner {
      Inner::Home { address, username } => match message {
        Message::Connect => {
          if let Some(ref mut conn) = &mut self.connection {
            info!("Connecting...");
            conn.try_send(conn::Input::Connect(username.clone(), address.parse().unwrap())).unwrap(/* FIXME: remove this */);
            self.inner = Inner::Connecting;
          }
        }
        Message::SetAddress(addr) => {
          *address = addr;
        }
        Message::SetUsername(usr) => {
          *username = usr;
        }
        Message::FontLoaded(_) => {}
        _ => {}
      },
      Inner::Connecting => {
        if let Message::Client(conn::Event::Connected) = message {
          self.inner = Inner::Connected {}
        }
      }
      Inner::Connected { .. } => match message {
        Message::Disconnect => {
          self.inner = Inner::default();
        }
        _ => {}
      },
    }
    Command::none()
  }

  fn view(&self) -> Element<Message> {
    match &self.inner {
      Inner::Home { address, username } => {
        let username = text_input("Username", username).on_input(Message::SetUsername);
        let conn_widget = row![
          text_input("Address", address).on_input(Message::SetAddress),
          button("Connect").on_press(Message::Connect)
        ];
        column![username, conn_widget].padding(20).into()
      }
      Inner::Connecting => text("Connecting...").into(),
      Inner::Connected { .. } => {
        let btn = button("Disconnect").on_press(Message::Disconnect);
        btn.into()
      }
    }
  }
}
