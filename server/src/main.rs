use clap::Parser;
use env_logger::Env;

mod config;
mod server;

#[derive(Parser, Debug)]
#[clap(name="Rust Voice Server")]
struct Args {
  #[clap(value_parser = clap::value_parser!(u16).range(1..), short='p', long="port", default_value_t=8080)]
  port: u16,
}

fn main() {
  env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

  let args = Args::parse();

  let config = config::ServerConfig {
    port: args.port,
    heartbeat_interval: std::time::Duration::from_secs(1),
    timeout: std::time::Duration::from_secs(3),
  };
  let mut server = server::Server::new(config);
  server.start();
}
