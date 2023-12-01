use std::time::Duration;

pub struct ServerConfig {
  pub port: u16,
  /// Time before a user is disconnected.
  pub timeout: Duration,
  /// Interval between heartbeat checks.
  pub heartbeat_interval: Duration,
}

impl ServerConfig {
  pub fn new() -> Self {
    Self {
      port: 8080,
      timeout: Duration::from_secs(100),
      heartbeat_interval: Duration::from_secs(1),
    }
  }
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self::new()
  }
}
