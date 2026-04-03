use std::time::Duration;

pub const FRAMERATE: f64 = 60.0;

pub fn tick_duration() -> Duration {
    Duration::from_secs_f64(1.0 / FRAMERATE)
}
