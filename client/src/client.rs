use std::net::{UdpSocket, ToSocketAddrs};

use common::packets;
use log::{debug, info, error};

pub struct Client {
  username: String,
  socket: UdpSocket,
  connected: bool,
}

impl Client {
  pub fn new(username: String) -> Self {
    Self {
      username,
      socket: UdpSocket::bind("0.0.0.0:0").unwrap(),
      connected: false,
    }
  }
  
  pub fn connect<A>(&self, addr: A) where A: ToSocketAddrs {
    self.socket.connect(addr).unwrap();
    self.send(packets::ClientMessage::Connect { username: self.username.clone() });
    info!("Connecting to {:?}...", self.socket.peer_addr().unwrap());
    let mut buf = [0; packets::PACKET_MAX_SIZE];
    match self.socket.recv(&mut buf) {
      Ok(bytes) => {
        let p = packets::ServerMessage::from_bytes(&buf[..bytes]).expect("Invalid packet from server.");
      },
      Err(e) => {
        error!("Failed to connect to server: {}", e);
      }
    }
  }

  pub fn send(&self, command: packets::ClientMessage) {
    let packet = bincode::serialize(&command).unwrap();
    self.socket.send(&packet).unwrap();
    debug!("-> {} bytes: {:?}", packet.len(), &packet);
  }

  
}