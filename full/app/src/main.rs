mod log_pipe;

use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::Arc;

use client::client::Client;
use client::services::{AudioService, OpusEncoder, PeerMixer};
use common::packets::AudioPacket;
use flexi_logger::{Logger, LoggerHandle, WriteMode};
use iced::widget::{button, column, row, text, text_input};
use iced::{
  executor, font, Alignment, Application, Command, Element, Font, Sandbox, Settings, Theme,
};
use log::info;
use log_pipe::LogPipe;

const FONT: Font = Font::with_name("Cabin");
pub fn main() -> anyhow::Result<()> {
  let pipe = LogPipe::new();
  let logger = Logger::try_with_str("debug")?
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
  Room { audio: AudioService, client: Client },
}
impl Default for Inner {
  fn default() -> Self {
    Self::Home {
      address: String::new(),
      username: String::new(),
    }
  }
}

struct App {
  log: LogPipe,
  logger: LoggerHandle,
  inner: Inner,
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
        inner: Inner::Home {
          address: String::new(),
          username: String::new(),
        },
      },
      font::load(include_bytes!("../fonts/Cabin-Regular.ttf").as_slice()).map(Message::FontLoaded),
    )
  }

  fn title(&self) -> String {
    String::from("rust-voice")
  }

  fn update(&mut self, message: Message) -> Command<Message> {
    match &mut self.inner {
      Inner::Home { address, username } => match message {
        Message::Connect => {
          info!("Connecting...");
          let mut audio = AudioService::builder().build().unwrap();
          let mixer = Arc::new(PeerMixer::new(
            audio.out_config().sample_rate.0,
            audio.out_latency(),
          ));
          audio.add_source(mixer.clone());
          audio.start().unwrap();

          let mic = audio
            .take_mic()
            .expect("could not take microphone from audio service");
          let mic = OpusEncoder::new(mic).expect("failed to create encoder");

          let (peer_tx, peer_rx) = channel::<AudioPacket<u8>>();
          let mut client = Client::new(username.to_string(), Arc::new(mic), peer_tx);
          client.connect(address.parse::<SocketAddr>().unwrap());
          self.inner = Inner::Room { audio, client };
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
      Inner::Room { .. } => match message {
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
      Inner::Room { audio, client } => {
        let btn = button("Disconnect").on_press(Message::Disconnect);
        btn.into()
      }
    }
  }
}
