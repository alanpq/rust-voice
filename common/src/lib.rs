pub mod packets;

mod atomic_counter;
mod user;

pub use atomic_counter::*;
pub use user::*;

pub type PeerID = u8; // max 256 peers
pub const MAX_PEERS: usize = u8::MAX as usize;
