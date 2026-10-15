//! Output/input device queries and change watching. Written in W1 Tasks 10–11.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    InputChanged,
    OutputChanged,
}

pub fn on_builtin_speakers() -> bool {
    false
}

pub fn others_playing() -> bool {
    false
}

pub struct DeviceWatcher;

impl DeviceWatcher {
    pub fn new(_on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        Ok(DeviceWatcher)
    }
}
