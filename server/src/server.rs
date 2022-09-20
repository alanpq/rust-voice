use std::{net::{UdpSocket, SocketAddr}, collections::{LinkedList, HashMap}, sync::{Arc, Mutex}, time::Instant};

use common::{packets::{self, ClientMessage, ServerMessage, LeaveReason}, UserInfo};
use log::{info, debug, error, warn};
use uuid::Uuid;

use crate::config::ServerConfig;

#[derive(Debug)]
#[derive(Clone)]
pub struct User {
  pub id: Uuid,
  pub username: String,
  pub addr: SocketAddr,
  pub last_reply: Instant,
}

impl User {
  pub fn info(&self) -> UserInfo {
    UserInfo {
      id: self.id,
      username: self.username.clone(),
    }
  }
}

pub struct Server {
  pub config: ServerConfig,
  socket: Option<UdpSocket>,
  users: Arc<Mutex<HashMap<SocketAddr,User>>>,
  running: bool,
}

impl Server {
  pub fn new(config: ServerConfig) -> Self {
    Server {
      config,
      socket: None,
      users: Arc::new(Mutex::new(HashMap::new())),
      running: false,
    }
  }


  pub fn start(&mut self) {
    if self.running {
      warn!("Server already running");
      return;
    }

    self.running = true;
    self.service();
  }
  
  fn handle_command(&self, addr: SocketAddr, command: ClientMessage) {
    let user = {
      let mut users = self.users.lock().unwrap();
      let mut user = users.get_mut(&addr);
      if let Some(user) = user.as_mut() {
        user.last_reply = Instant::now();
      }
      user.cloned()
    };
    match command {
      ClientMessage::Connect { username } => {
        if user.is_some() {
          error!("Connection from {} already exists", addr);
          return;
        }
        let mut users = self.users.lock().unwrap();
        let user = User {
          id: Uuid::new_v4(),
          username: username.clone(),
          addr,
          last_reply: Instant::now(),
        };
        info!("'{}' ({}) connected", &username, users.len());
        // TODO: change response from pong to something more important
        self.send(addr, ServerMessage::Pong).unwrap();
        for u in users.values() {
          self.send(user.addr, ServerMessage::Connected(u.info())).unwrap();
        }
        users.insert(addr, user.clone());
        info!("{} users connected", users.len());
        drop(users);
        self.broadcast(ServerMessage::Connected (user.info()), Some(addr));
      },
      ClientMessage::Disconnect => {
        if let Some(user) = user {
          let mut users = self.users.lock().unwrap();
          users.remove(&addr);
          info!("'{}' ({}) disconnected", &user.username, users.len());
          drop(users);
          self.broadcast(ServerMessage::Disconnected(user.info(), LeaveReason::Disconnect), None);
        }
      },
      ClientMessage::Ping => {
        if user.is_none() {return;}
        self.send(addr, ServerMessage::Pong).unwrap();
      },
      ClientMessage::Voice { samples } => {
        if user.is_none() {return;}
        self.broadcast(ServerMessage::Voice { user: user.unwrap().id, samples }, Some(addr));
        // self.broadcast(ServerMessage::Voice { user: user.unwrap().id, samples }, None);
      },
      _ => {}
    }
  }

  fn send(&self, addr: SocketAddr, command: ServerMessage) -> Result<usize, std::io::Error>{
    self.socket.as_ref().unwrap().send_to(&command.to_bytes(), addr)
  }

  fn broadcast(&self, command: ServerMessage, ignore: Option<SocketAddr>) {
    self.users.lock().unwrap().iter().for_each(|(addr, user)| {
      if Some(addr) == ignore.as_ref() {return;}
      self.send(*addr, command.clone()).unwrap();
    })
  }

  fn service(&mut self) {
    self.socket = Some(UdpSocket::bind(format!("0.0.0.0:{}", self.config.port))
      .expect("Failed to bind socket"));
    info!("Listening on port {}", self.config.port);

    let mut last_heartbeat = Instant::now();

    let socket = self.socket.as_ref().unwrap();
    socket.set_nonblocking(true).expect("Failed to set socket to non-blocking");

    loop {
      let mut buf = [0; packets::PACKET_MAX_SIZE];
      match socket.recv_from(&mut buf) {
        Ok((bytes, addr)) => {
          match packets::ClientMessage::from_bytes(&buf[..bytes]) {
            Some(command) => {
              self.handle_command(addr, command);
            }
            None => {
              error!("Failed to parse packet");
            }
          }
        }
        Err(e) => {
          match e.kind() {
            std::io::ErrorKind::WouldBlock => {
              if Instant::now().duration_since(last_heartbeat) <= self.config.heartbeat_interval { continue; }
              last_heartbeat = Instant::now();
              let mut users = self.users.lock().unwrap();

              let mut to_remove = Vec::new();
              for (addr, user) in users.iter() {
                if user.last_reply.elapsed() >= self.config.timeout {
                  info!("'{}' timed out.", user.username);
                  self.broadcast(ServerMessage::Disconnected(user.info(), LeaveReason::Timeout), None);
                  to_remove.push(*addr);
                }
              }
              for addr in to_remove {
                users.remove(&addr);
              }
            }
            _ => {
              error!("Failed to receive packet: {}", e);
            }
          };
        }
      }
    }
  }
}