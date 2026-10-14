//! Default device changes (AirPods in or out) and whether sound is coming out of the built-in speakers.

use super::coreaudio::*;
use block2::RcBlock;
use objc2_core_audio::*;
use std::ptr::NonNull;

const DATA_SOURCE_INTERNAL_SPEAKER: u32 = u32::from_be_bytes(*b"ispk");

/// True when the default output is the Mac's own speakers (not the headphone jack, AirPods, USB, HDMI...).
pub fn on_builtin_speakers() -> bool {
    let Ok(device) = default_output_device() else { return false };
    let transport: u32 = get(device, kAudioDevicePropertyTransportType, kAudioObjectPropertyScopeGlobal).unwrap_or(0);
    if transport != kAudioDeviceTransportTypeBuiltIn {
        return false;
    }
    // Built-in output with a data source: speakers vs. headphone jack. No data source: assume speakers.
    match get::<u32>(device, kAudioDevicePropertyDataSource, kAudioObjectPropertyScopeOutput) {
        Ok(source) => source == DATA_SOURCE_INTERNAL_SPEAKER,
        Err(_) => true,
    }
}

pub fn default_input_name() -> String {
    default_input_device().map(device_name).unwrap_or_else(|_| "No microphone".into())
}

pub fn default_output_name() -> String {
    default_output_device().map(device_name).unwrap_or_else(|_| "No output".into())
}

/// Something is playing through the default output (used to tell "silent call" from "blocked tap").
pub fn output_is_running() -> bool {
    default_output_device()
        .and_then(|d| get::<u32>(d, kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioObjectPropertyScopeGlobal))
        .map(|v| v != 0)
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    InputChanged,
    OutputChanged,
}

type ListenerBlock = RcBlock<dyn Fn(u32, NonNull<AudioObjectPropertyAddress>)>;

/// Listens for default input/output changes until dropped. The callback runs on a Core Audio thread.
pub struct DeviceWatcher {
    listeners: Vec<(AudioObjectPropertyAddress, ListenerBlock)>,
}

unsafe impl Send for DeviceWatcher {}

impl DeviceWatcher {
    pub fn new(on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        let on_change = std::sync::Arc::new(on_change);
        let mut listeners = vec![];
        for (selector, event) in [(kAudioHardwarePropertyDefaultInputDevice, DeviceEvent::InputChanged), (kAudioHardwarePropertyDefaultOutputDevice, DeviceEvent::OutputChanged)] {
            let a = addr(selector, kAudioObjectPropertyScopeGlobal);
            let cb = on_change.clone();
            let block: ListenerBlock = RcBlock::new(move |_n: u32, _a: NonNull<AudioObjectPropertyAddress>| cb(event));
            let status = unsafe { AudioObjectAddPropertyListenerBlock(kAudioObjectSystemObject as u32, NonNull::from(&a), None, RcBlock::as_ptr(&block) as AudioObjectPropertyListenerBlock) };
            check(status, "AudioObjectAddPropertyListenerBlock")?;
            listeners.push((a, block));
        }
        Ok(DeviceWatcher { listeners })
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        for (a, block) in &self.listeners {
            unsafe {
                AudioObjectRemovePropertyListenerBlock(kAudioObjectSystemObject as u32, NonNull::from(a), None, RcBlock::as_ptr(block) as AudioObjectPropertyListenerBlock);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_queries_do_not_fail() {
        let _ = on_builtin_speakers();
        let _ = output_is_running();
        assert!(!default_input_name().is_empty());
        assert!(!default_output_name().is_empty());
    }

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        let w = DeviceWatcher::new(|_| {}).unwrap();
        drop(w);
    }
}
