use std::{
  net::{ToSocketAddrs, UdpSocket},
  sync::{
    mpsc::{Receiver, Sender},
    Arc, Mutex,
  },
  time::Instant,
};

use crate::{
  services::OpusEncoder,
  source::{AudioByteSource, AudioSource},
};
use common::{
  packets::{self, AudioPacket, ServerMessage},
  UserInfo,
};
use crossbeam::channel;
use log::{debug, error, info, trace};
use tracing::{span, Level};

pub struct Client {
  username: String,
  socket: UdpSocket,
  connected: bool,

  mic: Arc<dyn AudioByteSource>,
  peer_tx: Arc<Mutex<Sender<AudioPacket<u8>>>>,

  peer_connected_tx: channel::Sender<UserInfo>,
  peer_connected_rx: channel::Receiver<UserInfo>,
}

impl Client {
  pub fn new(
    username: String,
    mic: Arc<dyn AudioByteSource>,
    peer_tx: Sender<AudioPacket<u8>>,
  ) -> Self {
    let (peer_connected_tx, peer_connected_rx) = channel::unbounded();
    Self {
      username,
      socket: UdpSocket::bind("0.0.0.0:0").unwrap(),
      connected: false,
      mic,
      peer_tx: Arc::new(Mutex::new(peer_tx)),

      peer_connected_tx,
      peer_connected_rx,
    }
  }

  pub fn get_peer_connected_rx(&self) -> channel::Receiver<UserInfo> {
    self.peer_connected_rx.clone()
  }

  pub fn connected(&self) -> bool {
    self.connected
  }

  pub fn server_addr(&self) -> String {
    self.socket.peer_addr().unwrap().to_string()
  }

  pub fn connect<A>(&mut self, addr: A)
  where
    A: ToSocketAddrs,
  {
    self.socket.connect(addr).unwrap();
    self.send(packets::ClientMessage::Connect {
      username: self.username.clone(),
    });
    info!("Connecting to {:?}...", self.socket.peer_addr().unwrap());
    let mut buf = [0; packets::PACKET_MAX_SIZE];
    match self.socket.recv(&mut buf) {
      Ok(bytes) => {
        let p =
          packets::ServerMessage::from_bytes(&buf[..bytes]).expect("Invalid packet from server.");
        match p {
          packets::ServerMessage::Pong => {
            info!("Connected to {:?}", self.socket.peer_addr().unwrap());
            self.connected = true;
          }
          _ => {
            error!("Unexpected packet from server: {:?}", p);
          }
        }
      }
      Err(e) => {
        error!("Failed to connect to server: {}", e);
      }
    }

    // self.socket.set_nonblocking(true);
  }

  pub async fn service(&self) {
    self
      .socket
      .set_nonblocking(true)
      .expect("Failed to set socket to non-blocking");
    let span = span!(Level::INFO, "client service");
    let mut seq_num = packets::SeqNum(0);
    loop {
      let _span = span.enter();
      let mut buf = [0; packets::PACKET_MAX_SIZE];
      match self.socket.recv(&mut buf) {
        Ok(bytes) => self.recv(&buf[..bytes]),
        Err(e) => {
          if e.kind() != std::io::ErrorKind::WouldBlock {
            error!("Failed to receive packet: {}", e);
            break;
          }
          if let Some(samples) = self.mic.next().await {
            trace!("sending voice packet size {}", samples.len());
            self.send(packets::ClientMessage::Voice { seq_num, samples });
            seq_num += 1;
          }
        }
      }
    }
  }

  fn recv(&self, buf: &[u8]) {
    // info!("Received {:?} bytes", buf.len());
    let command = packets::ServerMessage::from_bytes(buf).expect("Invalid packet from server.");
    match command {
      ServerMessage::Voice(packet) => {
        self.peer_tx.lock().unwrap().send(packet).unwrap();
      }
      ServerMessage::Connected(info) => {
        info!("{} connected.", info.username);
        self.peer_connected_tx.send(info).unwrap();
      }
      _ => {
        error!("Unexpected packet from server: {:?}", command);
      }
    }
  }

  pub fn send(&self, command: packets::ClientMessage) {
    let packet = bincode::serialize(&command).unwrap();
    self.socket.send(&packet).unwrap();
    trace!("-> {} bytes", packet.len());
  }
}
