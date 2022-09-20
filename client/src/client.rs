use std::{net::{UdpSocket, ToSocketAddrs}, sync::mpsc::Receiver, collections::VecDeque};

use common::packets::{self, ServerMessage};
use log::{debug, info, error};

use anyhow::anyhow;
use ringbuf::Consumer;
use uuid::Uuid;

pub enum ClientState {
  Invalid,
  Connecting,
  Connected,
  Disconnected,
}

const PACKET_MAX_SIZE: usize = 1024;

pub type OnVoiceCB = dyn Fn(Uuid, Vec<u8>) -> Result<(), anyhow::Error> + Send + Sync;
pub type OnDisconnect = dyn FnMut(Uuid) -> Result<(), anyhow::Error> + Send + Sync;
pub struct Client {
  username: String,
  socket: UdpSocket,
  state: ClientState,
  mic_rx: Receiver<Vec<u8>>,
}

impl Client {

  pub fn new(username: String, mic_rx: Receiver<Vec<u8>>) -> Result<Self, anyhow::Error> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    Ok(Self {
      username,
      socket,
      state: ClientState::Disconnected,
      mic_rx,
    })
  }

  pub fn connect<A>(&mut self, addr: A) -> Result<(), anyhow::Error> where A: ToSocketAddrs {
    let addr = addr.to_socket_addrs()?.next().ok_or_else(|| anyhow!("invalid address"))?;
    info!("Connecting to {:?}...", addr);
    self.socket.connect(addr)?;
    self.send(packets::ClientMessage::Connect { username: self.username.clone() })?;

    let pack = self.recv_packet()?;
    match pack {
      // TODO: change to ack packet
      Some(ServerMessage::Pong) => {
        self.state = ClientState::Connected;
        info!("Connected to {:?}", self.socket.peer_addr()?);
      },
      None => {},
      _ => error!("Connection failed: Unexpected packet received"),
    };
    self.socket.set_nonblocking(true)?;
    Ok(())
  }

  pub fn poll(&mut self) -> Result<Option<ServerMessage>, anyhow::Error> {
    let pack = self.recv_packet()?;
    if let Ok(packet) = self.mic_rx.try_recv() {
      self.send(packets::ClientMessage::Voice { samples: packet })?;
    }
    Ok(pack)
  }

  fn recv_packet(&self) -> Result<Option<ServerMessage>, anyhow::Error> {
    let mut buf = [0; 1024];
    match self.socket.recv(&mut buf) {
      Ok(size) => {
        // debug!("Received {} bytes", size);
        let packet = packets::ServerMessage::from_bytes(&buf[..size])
          .ok_or_else(|| anyhow!("Failed to parse packet"))?;
        Ok(Some(packet))
      },
      Err(e) => {
        if e.kind() == std::io::ErrorKind::WouldBlock {
          return Ok(None);
        }
        debug!("Error receiving packet: {}", e);
        Err(e.into())
      }
    }
  }

  pub fn send(&self, command: packets::ClientMessage) -> Result<(), anyhow::Error> {
    let packet = bincode::serialize(&command)?;
    self.socket.send(&packet)?;
    // debug!("-> {} bytes", packet.len());
    Ok(())
  }
}