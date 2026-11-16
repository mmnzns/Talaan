//! How much memory this PC has.

use ::windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// The PC's physical memory in bytes; None if Windows won't say.
pub fn total_memory() -> Option<u64> {
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(status.ullTotalPhys).filter(|&b| b > 0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_pc_has_some_memory() {
        assert!(super::total_memory().is_some_and(|b| b >= 1 << 30));
    }
}
