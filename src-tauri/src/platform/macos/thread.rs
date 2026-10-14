//! Thread priority for long background work (transcription), so a call being recorded never stutters.

const QOS_CLASS_UTILITY: u32 = 0x11;

unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
}

/// Puts the calling thread in the "utility" class: runs fully when the Mac is idle, yields to everything else.
pub fn background_priority() {
    unsafe {
        pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn setting_the_priority_does_not_crash() {
        std::thread::spawn(super::background_priority).join().unwrap();
    }
}
