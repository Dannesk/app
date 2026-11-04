
use std::sync::OnceLock;
use std::time::Instant;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn clock_secs() -> f32 {
    start().elapsed().as_secs_f32()
}
