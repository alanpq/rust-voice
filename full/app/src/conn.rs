use std::{any::TypeId, net::SocketAddr, sync::Arc};

use anyhow::Context;
use async_std::net::UdpSocket;
use common::{
  packets::{self, AudioPacket, ClientMessage, SeqNum, ServerMessage},
  UserInfo,
};
use futures::{channel::mpsc::Sender, FutureExt as _, StreamExt as _};
use iced::{
  futures::{channel::mpsc, lock::Mutex, SinkExt as _},
  subscription, Subscription,
};
use lib::{
  services::{AudioHandle, OpusEncoder, PeerMixer},
  source::AudioByteSource,
};
use log::{error, info, trace, warn};

use crate::{async_drop::Dropper, client::Client};

pub type Connection = mpsc::Sender<Input>;

#[derive(Debug, Clone)]
pub enum Event {
  Ready(Connection),
  Connected,
  Joined(UserInfo),
}

pub enum Input {
  Connect(String, SocketAddr),
  Disconnect,
}

pub enum State {
  Starting,
  Ready(Option<mpsc::Receiver<Input>>),
  Connected {
    audio: AudioHandle,
    mixer: Arc<PeerMixer>,
    mic: Arc<dyn AudioByteSource>,

    client: Dropper<Client>,
    rx: Option<mpsc::Receiver<Input>>,
  },
}

impl State {
  pub async fn run(&mut self, output: &mut Sender<Event>) {
    match self {
      State::Starting => {
        let (tx, rx) = mpsc::channel(128);
        let _ = output.send(Event::Ready(tx)).await;
        *self = State::Ready(Some(rx));
      }
      #[allow(clippy::single_match)]
      State::Ready(rx) => match rx.as_mut().unwrap().select_next_some().await {
        Input::Connect(username, addr) => {
          info!("Connecting...");
          let (audio, mic) = AudioHandle::builder().start().unwrap();
          audio.play();
          let mixer = Arc::new(PeerMixer::new(
            audio.out_cfg().sample_rate.0,
            audio.out_latency(),
          ));
          audio.add_source(mixer.clone());

          let mic = Arc::new(OpusEncoder::new(mic).expect("failed to create encoder"));

          let mut client = Client::new()
            .await
            .context("could not create client")
            .unwrap();
          client.connect(addr, username).await.unwrap();
          // info!("Connecting to {:?}...", socket.peer_addr().unwrap());

          info!("Connected!");
          let _ = output.send(Event::Connected).await;
          *self = State::Connected {
            audio,
            mixer,
            mic,
            client: client.into(),
            rx: rx.take(),
          }
        }
        _ => {}
      },
      State::Connected {
        audio,
        mixer,
        mic,
        client,
        rx,
      } => {
        futures::select! {
          res = client.next().fuse() => {match res {
            Ok(msg) => match msg {
              ServerMessage::Pong => {},
              ServerMessage::Connected(user) => {let _ = output.send(Event::Joined(user)).await;},
              ServerMessage::Voice(pak) => {
                mixer.push(pak.peer_id as u32, &pak.data);
              },
            },
            Err(e) => panic!("{e}"), // FIXME: dont fucking panic
          }}
          mic = mic.next().fuse() => {
            if let Some(samples) = mic {
              let seq_num = client.next_seq();
              client.send(ClientMessage::Voice { seq_num, samples }).await.unwrap();
            }
          }
          msg = rx.as_mut().unwrap().select_next_some() => {
            match msg {
              Input::Connect(_, _) => {},
              Input::Disconnect => {
                *self = State::Ready(rx.take());
              },
            }
          }
        }
      }
    }
  }
}

pub fn client() -> Subscription<Event> {
  struct Worker;
  subscription::channel(TypeId::of::<Worker>(), 128, |mut output| async move {
    let mut state = State::Starting;

    loop {
      state.run(&mut output).await;
    }
  })
}
