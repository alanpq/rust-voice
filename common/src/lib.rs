pub mod packets;

mod atomic_counter;
pub mod rolling_avg;
mod user;

pub use atomic_counter::*;
pub use rolling_avg::*;
pub use user::*;

pub type PeerID = u8; // max 256 peers
pub const MAX_PEERS: usize = u8::MAX as usize;
