mod config;
mod server;


fn main() {
  env_logger::builder().filter_level(log::LevelFilter::Debug).init();

  let config = config::ServerConfig::new();
  let mut server = server::Server::new(config);
  server.start();
}
