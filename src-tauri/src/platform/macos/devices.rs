//! Default device changes (AirPods in or out) and whether sound is coming out of the built-in speakers.

use super::coreaudio::*;
use block2::RcBlock;
use objc2_core_audio::*;
use std::ptr::NonNull;

const DATA_SOURCE_INTERNAL_SPEAKER: u32 = u32::from_be_bytes(*b"ispk");

/// True when call audio is coming out of the Mac's own speakers (not the headphone jack, AirPods, USB, HDMI...).
/// Either the default output is the built-in speakers, or a router such as eqMac or SoundSource is the default
/// and is driving the built-in speakers underneath (those report any transport type, so check the speakers themselves).
pub fn on_builtin_speakers() -> bool {
    let Ok(default) = default_output_device() else { return false };
    if is_builtin(default) {
        return is_internal_speaker(default);
    }
    let Ok(devices) = get_ids(kAudioObjectSystemObject as u32, kAudioHardwarePropertyDevices) else { return false };
    devices.into_iter().any(|d| {
        let running: u32 = get(d, kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioObjectPropertyScopeGlobal).unwrap_or(0);
        is_builtin(d) && has_output(d) && is_internal_speaker(d) && running != 0
    })
}

fn is_builtin(device: AudioObjectID) -> bool {
    get::<u32>(device, kAudioDevicePropertyTransportType, kAudioObjectPropertyScopeGlobal).unwrap_or(0) == kAudioDeviceTransportTypeBuiltIn
}

fn has_output(device: AudioObjectID) -> bool {
    let a = addr(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput);
    let mut size: u32 = 0;
    let status = unsafe { AudioObjectGetPropertyDataSize(device, NonNull::from(&a), 0, std::ptr::null(), NonNull::from(&mut size)) };
    status == 0 && size > 0
}

/// Built-in output with a data source: speakers vs. headphone jack. No data source: assume speakers.
fn is_internal_speaker(device: AudioObjectID) -> bool {
    match get::<u32>(device, kAudioDevicePropertyDataSource, kAudioObjectPropertyScopeOutput) {
        Ok(source) => source == DATA_SOURCE_INTERNAL_SPEAKER,
        Err(_) => true,
    }
}

/// Another app is playing sound (used to tell "the call is quiet" from "the tap is blocked").
/// Not the output device's running state: Talaan's own tap keeps that running.
pub fn others_playing() -> bool {
    other_process_playing()
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
        eprintln!("on built-in speakers: {}", on_builtin_speakers());
        let _ = others_playing();
    }

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        let w = DeviceWatcher::new(|_| {}).unwrap();
        drop(w);
    }
}
