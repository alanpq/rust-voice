pub trait AudioSource: Send + Sync {
    fn next(&self) -> Option<f32>;
}
