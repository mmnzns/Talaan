//! Small, safe-ish helpers over Core Audio property calls.

use objc2_core_audio::*;
use objc2_core_foundation::{CFRetained, CFString};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

pub fn addr(selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain }
}

pub fn check(status: i32, what: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        let code = status.to_be_bytes();
        let fourcc = if code.iter().all(|b| b.is_ascii_graphic()) { format!(" '{}'", String::from_utf8_lossy(&code)) } else { String::new() };
        Err(format!("{what} failed ({status}{fourcc})"))
    }
}

/// Reads a plain-old-data property.
/// `T` must be a plain C struct or number (all-zero bytes are a valid value).
pub fn get<T: Copy>(object: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope) -> Result<T, String> {
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = size_of::<T>() as u32;
    let a = addr(selector, scope);
    let status = unsafe {
        AudioObjectGetPropertyData(object, NonNull::from(&a), 0, std::ptr::null(), NonNull::from(&mut size), NonNull::from(&mut value).cast::<c_void>())
    };
    check(status, "AudioObjectGetPropertyData")?;
    Ok(value)
}

/// Reads a CFString property (device UID, name, ...).
pub fn get_string(object: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope) -> Result<String, String> {
    let mut raw: *const CFString = std::ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    let a = addr(selector, scope);
    let status = unsafe {
        AudioObjectGetPropertyData(object, NonNull::from(&a), 0, std::ptr::null(), NonNull::from(&mut size), NonNull::from(&mut raw).cast::<c_void>())
    };
    check(status, "AudioObjectGetPropertyData(string)")?;
    let ptr = NonNull::new(raw as *mut CFString).ok_or("empty string property")?;
    let s = unsafe { CFRetained::from_raw(ptr) };
    Ok(s.to_string())
}

pub fn default_output_device() -> Result<AudioObjectID, String> {
    get::<AudioObjectID>(kAudioObjectSystemObject as u32, kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal)
}

pub fn device_uid(device: AudioObjectID) -> Result<String, String> {
    get_string(device, kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal)
}

pub fn device_name(device: AudioObjectID) -> String {
    get_string(device, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal).unwrap_or_else(|_| "Unknown device".into())
}

/// Talaan's own Core Audio process object, so the tap can leave our own sounds out.
pub fn own_process_object() -> Option<AudioObjectID> {
    let pid: i32 = std::process::id() as i32;
    let mut object: AudioObjectID = 0;
    let mut size = size_of::<AudioObjectID>() as u32;
    let a = addr(kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyScopeGlobal);
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as u32,
            NonNull::from(&a),
            size_of::<i32>() as u32,
            &pid as *const i32 as *const c_void,
            NonNull::from(&mut size),
            NonNull::from(&mut object).cast::<c_void>(),
        )
    };
    (status == 0 && object != 0).then_some(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_output_device_has_a_uid_and_name() {
        let d = default_output_device().unwrap();
        assert!(!device_uid(d).unwrap().is_empty());
        assert!(!device_name(d).is_empty());
    }
}
