mod builder;
mod handle;
mod service;
mod stats;
mod streams;

pub use builder::*;
pub use handle::*;
pub use service::*;
pub use stats::*;

use crate::source::AudioSource;
use std::sync::{Arc, Mutex};

pub type AudioSources = Arc<Mutex<Vec<Arc<dyn AudioSource>>>>;
