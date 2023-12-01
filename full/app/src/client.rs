use std::net::SocketAddr;

use anyhow::{bail, Context};
use async_std::net::UdpSocket;
use async_trait::async_trait;
use common::packets::{self, ClientMessage, SeqNum, ServerMessage};
use log::{debug, trace};

use crate::async_drop::AsyncDrop;

pub struct Client {
  seq_num: SeqNum,
  socket: UdpSocket,

  buf: [u8; packets::PACKET_MAX_SIZE],
}

impl Client {
  pub async fn new() -> anyhow::Result<Client> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    Ok(Self {
      seq_num: SeqNum(0),
      socket,
      buf: [0; packets::PACKET_MAX_SIZE],
    })
  }

  pub async fn connect(&mut self, address: SocketAddr, username: String) -> anyhow::Result<()> {
    self.socket.connect(address).await?;
    self.send(ClientMessage::Connect { username }).await?;
    let ServerMessage::Pong = self.next().await? else {
      bail!("invalid ack from server");
    };
    Ok(())
  }

  // TODO: remove this once all packets have seq nums embedded
  pub fn next_seq(&mut self) -> SeqNum {
    let s = self.seq_num;
    self.seq_num += 1;
    s
  }

  pub async fn send(&self, msg: ClientMessage) -> anyhow::Result<()> {
    let pak = msg.to_bytes()?;
    self.socket.send(&pak).await?;
    trace!("-> {} bytes", pak.len());
    Ok(())
  }

  pub async fn next(&mut self) -> anyhow::Result<ServerMessage> {
    let bytes = self.socket.recv(&mut self.buf).await?;
    ServerMessage::from_bytes(&self.buf[..bytes]).context("invalid packet from server")
  }
}

#[async_trait]
impl AsyncDrop for Client {
  async fn async_drop(&mut self) {
    debug!("sending dc...");
    let _ = self.send(ClientMessage::Disconnect).await;
  }
}
