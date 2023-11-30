use std::{any::TypeId, net::SocketAddr, sync::Arc};

use async_std::net::UdpSocket;
use common::packets::{self, AudioPacket, SeqNum, ServerMessage};
use futures::FutureExt as _;
use iced::{
  futures::{channel::mpsc, lock::Mutex, SinkExt as _},
  subscription, Subscription,
};
use lib::{
  client::Client,
  services::{AudioHandle, OpusEncoder, PeerMixer},
  source::AudioByteSource,
};
use log::{error, info, trace, warn};

pub type Connection = mpsc::Sender<Input>;

#[derive(Debug, Clone)]
pub enum Event {
  Ready(Connection),
  Connected,
}

pub enum Input {
  Connect(String, SocketAddr),
  Disconnect,
}

pub enum State {
  Starting,
  Ready(mpsc::Receiver<Input>),
  Connected {
    audio: AudioHandle,
    mixer: Arc<PeerMixer>,

    seq_num: SeqNum,

    socket: UdpSocket,
    mic: Arc<dyn AudioByteSource>,
  },
}

async fn send(socket: &UdpSocket, command: common::packets::ClientMessage) {
  let packet = bincode::serialize(&command).unwrap();
  socket.send(&packet).await.unwrap();
  trace!("-> {} bytes", packet.len());
}

pub fn client() -> Subscription<Event> {
  struct Worker;
  subscription::channel(TypeId::of::<Worker>(), 128, |mut output| async move {
    let mut state = State::Starting;

    loop {
      match &mut state {
        State::Starting => {
          let (tx, rx) = mpsc::channel(128);
          let _ = output.send(Event::Ready(tx)).await;
          state = State::Ready(rx);
        }
        State::Ready(rx) => {
          use iced::futures::StreamExt;
          match rx.select_next_some().await {
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

              let socket = UdpSocket::bind("0.0.0.0:0")
                .await
                .expect("could not bind socket");
              socket
                .connect(addr)
                .await
                .expect("TODO: socket failed to connect");

              send(
                &socket,
                packets::ClientMessage::Connect {
                  username: username.clone(),
                },
              )
              .await;

              info!("Connecting to {:?}...", socket.peer_addr().unwrap());
              let mut buf = [0; packets::PACKET_MAX_SIZE];

              match socket.recv(&mut buf).await {
                Ok(bytes) => {
                  let p = packets::ServerMessage::from_bytes(&buf[..bytes])
                    .expect("invalid packet from server");
                  match p {
                    packets::ServerMessage::Pong => {
                      info!("Connected!");
                      let _ = output.send(Event::Connected).await;
                      state = State::Connected {
                        audio,
                        mixer,
                        seq_num: SeqNum(0),
                        mic,
                        socket,
                      }
                    }
                    _ => {
                      error!("Unexpected packet from server: {p:?}");
                    }
                  }
                }
                Err(_) => todo!(),
              }
            }
            Input::Disconnect => todo!(),
          }
        }
        State::Connected {
          audio,
          mixer,
          seq_num,
          mic,
          socket,
        } => {
          let mut buf = [0; packets::PACKET_MAX_SIZE];
          futures::select! {
            res = socket.recv(&mut buf).fuse() => {
              if let Ok(bytes) = res {
                let msg = ServerMessage::from_bytes(&buf[..bytes]).expect("invalid packet from server");
                match msg {
                  ServerMessage::Voice(packet) => {
                    // self.peer_tx.lock().unwrap().send(packet).unwrap();
                    mixer.push(packet.peer_id as u32, &packet.data);
                  }
                  ServerMessage::Connected(info) => {
                    info!("{} connected.", info.username);
                    // self.peer_connected_tx.send(info).unwrap();
                  }
                  _ => {
                    error!("Unexpected packet from server: {:?}", msg);
                  }
                }
              }
            }
            mic = mic.next().fuse() => {
              if let Some(samples) = mic {
                send(socket, packets::ClientMessage::Voice { seq_num: *seq_num, samples }).await;
                *seq_num += 1;
              }
            }
          }
        }
      }
    }
  })
}
