mod handle;
use std::sync::{Arc, Mutex};

pub use handle::*;

mod service;
pub use service::*;

mod builder;
pub use builder::*;

mod streams;

use crate::source::AudioSource;

pub type AudioSources = Arc<Mutex<Vec<Arc<dyn AudioSource>>>>;
