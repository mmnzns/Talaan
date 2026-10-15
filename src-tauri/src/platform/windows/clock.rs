//! Host time in nanoseconds: QueryPerformanceCounter on the 100 ns grid WASAPI uses for its QPC positions,
//! so cpal's mic timestamps and the loopback timestamps are on the same clock as `host_now_ns`.

use ::windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub fn host_now_ns() -> u64 {
    let (mut counter, mut freq) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
        let _ = QueryPerformanceFrequency(&mut freq);
    }
    qpc_to_ns(counter as u64, freq as u64)
}

/// A QPC reading -> ns, through 100 ns units (matches cpal's `now()` and WASAPI positions exactly).
pub fn qpc_to_ns(counter: u64, freq: u64) -> u64 {
    if freq == 0 {
        return 0;
    }
    (counter as u128 * 10_000_000 / freq as u128 * 100) as u64
}

/// A WASAPI QPC position (100 ns units) -> ns.
pub fn hns_to_ns(hns: u64) -> u64 {
    hns.saturating_mul(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpc_counts_become_nanoseconds_on_the_100ns_grid() {
        assert_eq!(qpc_to_ns(10_000_000, 10_000_000), 1_000_000_000);
        assert_eq!(qpc_to_ns(3, 10_000_000), 300); // 3 ticks of 100 ns
        assert_eq!(qpc_to_ns(1, 3_000_000), 300); // 333 ns floors to 300, like cpal's now()
        assert_eq!(qpc_to_ns(5, 0), 0); // never divides by zero
    }

    #[test]
    fn wasapi_positions_are_100ns_units() {
        assert_eq!(hns_to_ns(5), 500);
    }

    #[test]
    fn host_time_moves_forward_in_nanoseconds() {
        let a = host_now_ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let d = host_now_ns() - a;
        assert!((15_000_000..200_000_000).contains(&d), "{d}");
    }
}

