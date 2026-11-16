//! How much memory this Mac has.

/// The Mac's physical memory in bytes (`hw.memsize`); None if the system won't say.
pub fn total_memory() -> Option<u64> {
    let mut bytes = 0u64;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name is a NUL-terminated string, and `bytes` and `len` describe an 8-byte buffer the call fills in.
    let rc = unsafe { libc::sysctlbyname(c"hw.memsize".as_ptr(), &mut bytes as *mut u64 as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    (rc == 0 && len == std::mem::size_of::<u64>() && bytes > 0).then_some(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_mac_has_some_memory() {
        assert!(super::total_memory().is_some_and(|b| b >= 1 << 30));
    }
}
