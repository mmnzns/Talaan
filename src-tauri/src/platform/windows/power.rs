//! Sleep and wake watching, for auto-pause. Written in W1 Task 12.

pub struct SleepWatcher;

impl SleepWatcher {
    pub fn new(_on_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        SleepWatcher
    }
}
