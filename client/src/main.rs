use client::Client;

mod client;

fn main() -> Result<(), anyhow::Error> {
  env_logger::builder().filter_level(log::LevelFilter::Debug).init();

  let client = Client::new("test".to_string());
  client.connect("127.0.0.1:8080");
  
  Ok(())
}