use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{atomic::AtomicUsize, Arc},
  time::Instant,
};

use common::{
  packets::{self, AudioPacket, ClientMessage, ServerMessage},
  UserInfo,
};
use log::{debug, error, info, trace, warn};
use tokio::{net::UdpSocket, select, sync::Mutex, time};

use crate::config::ServerConfig;

#[derive(Debug, Clone)]
pub struct User {
  pub id: u32,
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
  users: Arc<Mutex<HashMap<SocketAddr, User>>>,
  counter: Arc<AtomicUsize>,
  running: bool,
}

impl Server {
  pub fn new(config: ServerConfig) -> Self {
    Server {
      config,
      socket: None,
      users: Arc::new(Mutex::new(HashMap::new())),
      counter: Arc::new(AtomicUsize::new(0)),
      running: false,
    }
  }

  pub async fn start(&mut self) {
    if self.running {
      warn!("Server already running");
      return;
    }

    self.running = true;
    self.service().await;
  }

  async fn handle_command(&self, addr: SocketAddr, command: ClientMessage) {
    let user = {
      let mut users = self.users.lock().await;
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
        let mut users = self.users.lock().await;
        let id = self
          .counter
          .fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u32;
        let user = User {
          id,
          username: username.clone(),
          addr,
          last_reply: Instant::now(),
        };
        info!("'{}' ({}) connected", &username, id);
        // TODO: change response from pong to something more important
        self.send(addr, ServerMessage::Pong).await.unwrap();
        for u in users.values() {
          self
            .send(user.addr, ServerMessage::Connected(u.info()))
            .await
            .unwrap();
        }
        users.insert(addr, user.clone());
        info!("{} users connected", users.len());
        debug!("{users:?}");
        drop(users);
        self
          .broadcast(ServerMessage::Connected(user.info()), Some(addr))
          .await;
      }
      ClientMessage::Disconnect => {
        let Some(user) = user else {
          return;
        };
        {
          let mut users = self.users.lock().await;
          users.remove(&user.addr);
          info!("'{}' left.", user.username);
          info!("{} users connected", users.len());
          debug!("{users:?}");
        }
        self
          .broadcast(ServerMessage::Disconnected(user.info()), None)
          .await;
      }
      ClientMessage::Ping => {
        if user.is_none() {
          return;
        }
        self.send(addr, ServerMessage::Pong).await.unwrap();
      }
      ClientMessage::Voice { seq_num, samples } => {
        let Some(user) = user else {
          return;
        };
        self
          .broadcast(
            ServerMessage::Voice(AudioPacket {
              seq_num,
              peer_id: user.id as u8,
              data: samples,
            }),
            Some(addr),
          )
          .await; //, Some(addr));
                  // self.broadcast(ServerMessage::Voice { user: user.unwrap().id, samples }, None);
      }
    }
  }

  async fn send(&self, addr: SocketAddr, command: ServerMessage) -> Result<usize, std::io::Error> {
    self
      .socket
      .as_ref()
      .unwrap()
      .send_to(&command.to_bytes(), addr)
      .await
  }

  async fn broadcast(&self, command: ServerMessage, ignore: Option<SocketAddr>) {
    trace!("broadcast: {command:?}");
    for (addr, _user) in self.users.lock().await.iter() {
      if Some(addr) == ignore.as_ref() {
        trace!(" - ignoring '{addr}'");
        continue;
      }
      self.send(*addr, command.clone()).await.unwrap();
    }
  }

  async fn service(&mut self) {
    self.socket = Some(
      UdpSocket::bind(format!("0.0.0.0:{}", self.config.port))
        .await
        .expect("Failed to bind socket"),
    );
    info!("Listening on port {}", self.config.port);

    let mut heartbeat = time::interval(self.config.heartbeat_interval);
    heartbeat.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    let socket = self.socket.as_ref().unwrap();

    loop {
      let mut buf = [0; packets::PACKET_MAX_SIZE];
      select! {
        bytes = socket.recv_from(&mut buf) => {
          match bytes {
            Ok((bytes, addr)) => match packets::ClientMessage::from_bytes(&buf[..bytes]) {
              Some(command) => {
                self.handle_command(addr, command).await;
              }
              None => {
                error!("Failed to parse packet");
              }
            },
            Err(e) => {
              error!("{e}");
            }
          }
        }
        _ = heartbeat.tick() => {
          if let Ok(mut users) = self.users.try_lock() {
            let user_count = users.len();
            users.retain(|_, user| user.last_reply.elapsed() < self.config.timeout);
            if users.len() < user_count {
              // did we lose users
              info!(
                "{} users lost connection. ({} users connected)",
                user_count - users.len(),
                users.len()
              );
            }
          }
        }
      }
    }
  }
}
