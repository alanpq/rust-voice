use std::{net::{UdpSocket, ToSocketAddrs}, sync::mpsc::{Sender, Receiver}, time::Instant};

use common::packets::{self, ServerMessage};
use log::{debug, info, error};

pub struct Client {
  username: String,
  socket: UdpSocket,
  connected: bool,

  mic_rx: Receiver<Vec<i16>>,
  peer_tx: Sender<(u32, Vec<i16>)>,
}

impl Client {
  pub fn new(username: String, mic_rx: Receiver<Vec<i16>>, peer_tx: Sender<(u32,Vec<i16>)>) -> Self {
    Self {
      username,
      socket: UdpSocket::bind("0.0.0.0:0").unwrap(),
      connected: false,
      mic_rx,
      peer_tx,
    }
  }

  pub fn connected(&self) -> bool { self.connected }
  
  pub fn connect<A>(&mut self, addr: A) where A: ToSocketAddrs {
    self.socket.connect(addr).unwrap();
    self.send(packets::ClientMessage::Connect { username: self.username.clone() });
    info!("Connecting to {:?}...", self.socket.peer_addr().unwrap());
    let mut buf = [0; packets::PACKET_MAX_SIZE];
    match self.socket.recv(&mut buf) {
      Ok(bytes) => {
        let p = packets::ServerMessage::from_bytes(&buf[..bytes]).expect("Invalid packet from server.");
        match p {
          packets::ServerMessage::Pong => {
            info!("Connected to {:?}", self.socket.peer_addr().unwrap());
            self.connected = true;
          }
          _ => {
            error!("Unexpected packet from server: {:?}", p);
          }
        }
      },
      Err(e) => {
        error!("Failed to connect to server: {}", e);
      }
    }

    // self.socket.set_nonblocking(true);
  }

  pub fn service(&mut self) {
    self.socket.set_nonblocking(true).expect("Failed to set socket to non-blocking");
    let mut last_sent_voice = Instant::now();
    loop {
      let mut buf = [0; packets::PACKET_MAX_SIZE];
      match self.socket.recv(&mut buf) {
        Ok(bytes) => self.recv(&buf[..bytes]),
        Err(e) => {
          if e.kind() != std::io::ErrorKind::WouldBlock {
            error!("Failed to receive packet: {}", e);
            break;
          }
          if Instant::now().duration_since(last_sent_voice) <= std::time::Duration::from_millis(500) {
            last_sent_voice = Instant::now();
            match self.mic_rx.try_recv() {
              Ok(samples) => {
                self.send(packets::ClientMessage::Voice { samples });
              }
              Err(e) => {
                if e == std::sync::mpsc::TryRecvError::Empty { continue; }
                error!("Failed to receive samples: {}", e);
                break;
              }
            }
          }
        }
      }
    }
  }

  fn recv(&mut self, buf: &[u8]) {
    // info!("Received {:?} bytes", buf.len());
    let command = packets::ServerMessage::from_bytes(buf).expect("Invalid packet from server.");
    match command {
      ServerMessage::Voice { username, samples } => {
        self.peer_tx.send((0, samples)).unwrap();
      },
      _ => {
        error!("Unexpected packet from server: {:?}", command);
      }
    }
  }

  pub fn send(&self, command: packets::ClientMessage) {
    let packet = bincode::serialize(&command).unwrap();
    self.socket.send(&packet).unwrap();
    debug!("-> {} bytes", packet.len());
  }

  
}