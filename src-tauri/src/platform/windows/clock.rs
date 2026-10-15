//! The host clock in nanoseconds. A temporary wall-clock stub; QPC arrives in W0 Task 5.

pub fn host_now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}
