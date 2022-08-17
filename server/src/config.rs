use crate::server::Server;

pub struct ServerConfig {
  pub port: u16,
}

impl ServerConfig {
  pub fn new() -> Self {
    Self {
      port: 8080,
    }
  }
}