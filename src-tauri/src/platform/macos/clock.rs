//! Host time in nanoseconds: mach absolute time, the same clock cpal and Core Audio stamp buffers with.

pub fn host_now_ns() -> u64 {
    let mut info = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
    unsafe {
        mach2::mach_time::mach_timebase_info(&mut info);
        let t = mach2::mach_time::mach_absolute_time() as u128;
        (t * info.numer as u128 / info.denom.max(1) as u128) as u64
    }
}

/// Converts a Core Audio `mHostTime` (mach ticks) to nanoseconds.
pub fn host_ticks_to_ns(ticks: u64) -> u64 {
    let mut info = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
    unsafe { mach2::mach_time::mach_timebase_info(&mut info) };
    (ticks as u128 * info.numer as u128 / info.denom.max(1) as u128) as u64
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_time_moves_forward_in_nanoseconds() {
        let a = super::host_now_ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let d = super::host_now_ns() - a;
        assert!((15_000_000..200_000_000).contains(&d), "{d}");
    }
}
