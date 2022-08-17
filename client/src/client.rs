use std::net::{UdpSocket, ToSocketAddrs};

use common::packets;
use log::debug;

pub struct Client {
  username: String,
  socket: UdpSocket,
}

impl Client {
  pub fn new(username: String) -> Self {
    Self {
      username,
      socket: UdpSocket::bind("0.0.0.0:0").unwrap()
    }
  }
  
  pub fn connect<A>(&self, addr: A) where A: ToSocketAddrs {
    self.socket.connect(addr).unwrap();
    self.send(packets::ClientMessage::Connect { username: self.username.clone() })
  }

  pub fn send(&self, command: packets::ClientMessage) {
    let packet = bincode::serialize(&command).unwrap();
    self.socket.send(&packet).unwrap();
    debug!("-> {} bytes: {:?}", packet.len(), &packet);
  }
}