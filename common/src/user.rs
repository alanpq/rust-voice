use serde::{Serialize, Deserialize};

#[derive(Clone)]
#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
  pub id: u32,
  pub username: String,
}