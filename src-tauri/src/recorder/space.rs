//! Warning before the disk fills: how long the free space lasts at the recording rate. Pure.

use super::clock::RATE;
use std::time::{Duration, Instant};

/// Both tracks are 16 kHz mono 16-bit (2 bytes a sample): 64,000 bytes a second, about 230 MB an hour.
pub const BYTES_PER_SECOND: u64 = RATE as u64 * 2 * 2;

/// The warning comes when less than this much recording fits.
const WARN_BELOW_SECONDS: u64 = 30 * 60;

/// The minutes of recording left to tell the user about, or None when there is room or they were already told.
pub fn low_space_minutes(free_bytes: u64, bytes_per_second: u64, already_warned: bool) -> Option<u64> {
    if already_warned || bytes_per_second == 0 {
        return None;
    }
    let seconds = free_bytes / bytes_per_second;
    (seconds < WARN_BELOW_SECONDS).then_some(seconds / 60)
}

/// How often the free space is looked at while recording.
const CHECK_EVERY: Duration = Duration::from_secs(30);

/// One recording's free-space watch: when to look, and the one warning.
#[derive(Default)]
pub struct DiskWatch {
    last: Option<Instant>,
    warned: bool,
}

impl DiskWatch {
    /// True the first time (the start of the recording), then every 30 seconds.
    pub fn due(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|t| now.duration_since(t) < CHECK_EVERY) {
            return false;
        }
        self.last = Some(now);
        true
    }

    /// The minutes to warn about for what `free_space` found, once per recording. An unknown amount says nothing.
    pub fn check(&mut self, free_bytes: Option<u64>) -> Option<u64> {
        let minutes = low_space_minutes(free_bytes?, BYTES_PER_SECOND, self.warned)?;
        self.warned = true;
        Some(minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_is_two_16_khz_16_bit_tracks() {
        assert_eq!(BYTES_PER_SECOND, 64_000);
        assert_eq!(BYTES_PER_SECOND * 3600 / 1_000_000, 230); // MB an hour
    }

    #[test]
    fn lots_of_room_says_nothing() {
        assert_eq!(low_space_minutes(50_000_000_000, BYTES_PER_SECOND, false), None);
        // exactly 30 minutes is still enough
        assert_eq!(low_space_minutes(BYTES_PER_SECOND * 30 * 60, BYTES_PER_SECOND, false), None);
    }

    #[test]
    fn under_half_an_hour_warns_with_the_minutes_left() {
        assert_eq!(low_space_minutes(BYTES_PER_SECOND * 30 * 60 - 1, BYTES_PER_SECOND, false), Some(29));
        assert_eq!(low_space_minutes(BYTES_PER_SECOND * 7 * 60 + 5, BYTES_PER_SECOND, false), Some(7));
        assert_eq!(low_space_minutes(1_000, BYTES_PER_SECOND, false), Some(0));
        assert_eq!(low_space_minutes(0, BYTES_PER_SECOND, false), Some(0));
    }

    #[test]
    fn it_warns_once() {
        assert_eq!(low_space_minutes(1_000, BYTES_PER_SECOND, true), None);
    }

    #[test]
    fn a_zero_rate_never_divides() {
        assert_eq!(low_space_minutes(1_000, 0, false), None);
    }

    #[test]
    fn the_watch_looks_at_the_start_then_every_thirty_seconds() {
        let mut w = DiskWatch::default();
        let t0 = Instant::now();
        assert!(w.due(t0));
        assert!(!w.due(t0 + Duration::from_millis(100)));
        assert!(!w.due(t0 + Duration::from_secs(29)));
        assert!(w.due(t0 + Duration::from_secs(30)));
        assert!(!w.due(t0 + Duration::from_secs(45)));
        assert!(w.due(t0 + Duration::from_secs(60)));
    }

    #[test]
    fn the_watch_warns_once_and_ignores_an_unknown_amount() {
        let mut w = DiskWatch::default();
        assert_eq!(w.check(None), None);
        assert_eq!(w.check(Some(50_000_000_000)), None);
        assert_eq!(w.check(Some(BYTES_PER_SECOND * 600)), Some(10));
        assert_eq!(w.check(Some(BYTES_PER_SECOND * 300)), None, "already told");
        assert_eq!(DiskWatch::default().check(Some(BYTES_PER_SECOND * 300)), Some(5), "a new recording tells again");
    }
}
