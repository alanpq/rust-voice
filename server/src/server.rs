use std::{net::UdpSocket, collections::LinkedList, sync::{Arc, Mutex}};

use common::packets::{self, ClientMessage};
use log::{info, debug, error};

use crate::config::ServerConfig;

pub struct User {
  pub username: String,
  pub udp_socket: Option<UdpSocket>
}

pub struct Server {
  pub config: ServerConfig,
  users: Arc<Mutex<LinkedList<User>>>,
  running: bool,
}

impl Server {
  pub fn new(config: ServerConfig) -> Self {
    Server {
      config,
      users: Arc::new(Mutex::new(LinkedList::new())),
      running: false,
    }
  }

  pub fn start(&mut self) {
    if self.running {
      println!("Server already running");
      return;
    }

    self.running = true;
    self.service();
  }
  
  fn handle_command(&self, command: ClientMessage) {
    debug!("got command: {:?}", command);
  }

  fn service(&self) {
    let sock = UdpSocket::bind(format!("0.0.0.0:{}", self.config.port))
      .expect("Failed to bind socket");
    info!("Listening on port {}", self.config.port);

    loop {
      let mut buf = [0; packets::PACKET_MAX_SIZE];
      if let Ok(bytes) = sock.recv(&mut buf) {
        info!("<- {} bytes", bytes);
        // debug!("{:?}", &buf[..bytes]);
        match bincode::deserialize::<ClientMessage>(&buf[..bytes]) {
          Ok(command) => self.handle_command(command),
          Err(e) => {
            error!("Failed to deserialize packet: {}", e);
          }
        }
      }
    }
  }
}