//! The recording clock: host time (ns) to position on the recording timeline, which excludes paused time.

pub const RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct Clock {
    start_ns: u64,
    /// Finished pauses as host-time intervals, in order.
    pauses: Vec<(u64, u64)>,
    paused_since: Option<u64>,
}

impl Clock {
    pub fn new(start_ns: u64) -> Self {
        Clock { start_ns, pauses: vec![], paused_since: None }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_since.is_some()
    }

    fn paused_before(&self, host_ns: u64) -> u64 {
        self.pauses.iter().filter(|(_, e)| *e <= host_ns).map(|(s, e)| e - s).sum()
    }

    /// Nanoseconds on the recording timeline, or `None` if `host_ns` is before the start or inside a pause.
    pub fn position_ns(&self, host_ns: u64) -> Option<u64> {
        if host_ns < self.start_ns {
            return None;
        }
        if self.paused_since.is_some_and(|p| host_ns >= p) {
            return None;
        }
        if self.pauses.iter().any(|(s, e)| host_ns >= *s && host_ns < *e) {
            return None;
        }
        Some(host_ns - self.start_ns - self.paused_before(host_ns))
    }

    /// Position in 16 kHz samples.
    pub fn position_samples(&self, host_ns: u64) -> Option<u64> {
        self.position_ns(host_ns).map(|ns| ns * RATE as u64 / 1_000_000_000)
    }

    /// Elapsed recording time in ms (frozen while paused).
    pub fn elapsed_ms(&self, now_ns: u64) -> u64 {
        let at = self.paused_since.unwrap_or(now_ns).max(self.start_ns);
        (at - self.start_ns - self.paused_before(at)) / 1_000_000
    }

    /// Returns the recording-clock ms where the pause begins, or `None` if already paused.
    pub fn pause(&mut self, now_ns: u64) -> Option<u64> {
        if self.paused_since.is_some() {
            return None;
        }
        let at = self.elapsed_ms(now_ns);
        self.paused_since = Some(now_ns);
        Some(at)
    }

    /// Returns (recording-clock ms of the pause, paused duration ms), or `None` if not paused.
    pub fn resume(&mut self, now_ns: u64) -> Option<(u64, u64)> {
        let since = self.paused_since.take()?;
        let end = now_ns.max(since);
        let at = (since - self.start_ns - self.paused_before(since)) / 1_000_000;
        self.pauses.push((since, end));
        Some((at, (end - since) / 1_000_000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const S: u64 = 1_000_000_000;

    #[test]
    fn positions_count_from_the_start() {
        let c = Clock::new(10 * S);
        assert_eq!(c.position_ns(9 * S), None);
        assert_eq!(c.position_ns(10 * S), Some(0));
        assert_eq!(c.position_samples(12 * S), Some(32_000));
        assert_eq!(c.elapsed_ms(12 * S), 2_000);
    }

    #[test]
    fn pauses_are_cut_out_of_the_timeline() {
        let mut c = Clock::new(0);
        assert_eq!(c.pause(5 * S), Some(5_000));
        assert_eq!(c.position_ns(6 * S), None);
        assert_eq!(c.elapsed_ms(100 * S), 5_000, "frozen while paused");
        assert_eq!(c.resume(65 * S), Some((5_000, 60_000)));
        assert_eq!(c.position_ns(64 * S), None, "inside the finished pause");
        assert_eq!(c.position_ns(65 * S), Some(5 * S));
        assert_eq!(c.position_ns(70 * S), Some(10 * S));
        assert_eq!(c.elapsed_ms(70 * S), 10_000);
    }

    #[test]
    fn a_buffer_captured_before_the_pause_keeps_its_position() {
        let mut c = Clock::new(0);
        c.pause(5 * S);
        c.resume(10 * S);
        assert_eq!(c.position_ns(4 * S), Some(4 * S));
    }

    #[test]
    fn double_pause_and_resume_without_pause_are_ignored() {
        let mut c = Clock::new(0);
        assert!(c.resume(1 * S).is_none());
        c.pause(2 * S);
        assert!(c.pause(3 * S).is_none());
        assert_eq!(c.resume(4 * S), Some((2_000, 2_000)));
    }

    #[test]
    fn several_pauses_add_up() {
        let mut c = Clock::new(0);
        c.pause(10 * S);
        c.resume(20 * S);
        c.pause(30 * S);
        assert_eq!(c.resume(40 * S), Some((20_000, 10_000)));
        assert_eq!(c.position_ns(50 * S), Some(30 * S));
    }
}
