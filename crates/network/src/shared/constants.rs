use std::time::Duration;

pub const FRAMERATE: f64 = 30.0;

pub fn tick_duration() -> Duration {
    Duration::from_secs_f64(1.0 / FRAMERATE)
}
