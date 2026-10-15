//! Calendar access and events. Written in W4 (spike: WinRT AppointmentStore, else Outlook).

use crate::calendar::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    NotAsked,
    Denied,
}

pub fn access() -> Access {
    Access::NotAsked
}

pub fn request() -> Access {
    Access::Denied
}

pub fn events(_from_ms: i64, _to_ms: i64) -> Vec<Event> {
    vec![]
}
