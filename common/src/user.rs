use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Clone)]
#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
  pub id: Uuid,
  pub username: String,
}