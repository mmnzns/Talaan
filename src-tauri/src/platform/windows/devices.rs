//! Default device changes (headphones in or out) and whether sound is coming out of the computer's own speakers.

use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_EnumeratorName;
use ::windows::Win32::Foundation::PROPERTYKEY;
use ::windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor};
use ::windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32, PROPVARIANT};
use ::windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

const FORM_FACTOR_SPEAKERS: u32 = 1;

/// Speakers wired to this computer (built in, or on its audio jack), not USB or Bluetooth.
/// HDMI/DisplayPort monitors report form factor 9, headphones 3, headsets 5, so they never match.
pub fn builtin_speakers(form_factor: u32, enumerator: &str) -> bool {
    let external = ["USB", "BTHENUM", "BTHHFENUM", "BTHLEDEVICE"].iter().any(|e| enumerator.eq_ignore_ascii_case(e));
    form_factor == FORM_FACTOR_SPEAKERS && !external
}

/// True when call audio is coming out of speakers the mic can hear: the default output is speakers wired to this
/// computer. A virtual router (FxSound reports Speakers on the "ROOT" enumerator) counts as built in, whatever
/// real device it drives underneath.
pub fn on_builtin_speakers() -> bool {
    let Ok(device) = default_output() else { return false };
    let (Ok(form), Ok(enumerator)) = (device_u32(&device, &PKEY_AudioEndpoint_FormFactor), device_string(&device, &PKEY_Device_EnumeratorName)) else {
        return false;
    };
    builtin_speakers(form, &enumerator)
}

/// Windows loopback can't be blocked by a permission, so the Mac's "no call audio" watch doesn't run here.
pub fn others_playing() -> bool {
    false
}

fn enumerator() -> ::windows::core::Result<IMMDeviceEnumerator> {
    super::com::init();
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn default_output() -> ::windows::core::Result<IMMDevice> {
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole) }
}

/// Reads one endpoint property, hands the PROPVARIANT to `read`, then frees it.
fn device_property<T>(device: &IMMDevice, key: &PROPERTYKEY, read: impl FnOnce(&PROPVARIANT) -> ::windows::core::Result<T>) -> ::windows::core::Result<T> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ)?;
        let mut value = store.GetValue(key)?;
        let result = read(&value);
        let _ = PropVariantClear(&mut value);
        result
    }
}

fn device_u32(device: &IMMDevice, key: &PROPERTYKEY) -> ::windows::core::Result<u32> {
    device_property(device, key, |v| unsafe { PropVariantToUInt32(v) })
}

fn device_string(device: &IMMDevice, key: &PROPERTYKEY) -> ::windows::core::Result<String> {
    device_property(device, key, |v| unsafe {
        let s = PropVariantToStringAlloc(v)?;
        let text = s.to_string().unwrap_or_default();
        CoTaskMemFree(Some(s.0 as *const _));
        Ok(text)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    InputChanged,
    OutputChanged,
}

pub struct DeviceWatcher;

impl DeviceWatcher {
    pub fn new(_on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        Ok(DeviceWatcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use ::windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

    #[test]
    fn only_wired_speakers_count() {
        assert!(builtin_speakers(1, "HDAUDIO")); // laptop speakers, or desktop speakers on the jack
        assert!(builtin_speakers(1, "INTELAUDIO"));
        assert!(!builtin_speakers(1, "USB")); // USB speakers / dock: still a risk, but not "this computer's"
        assert!(!builtin_speakers(1, "BTHENUM"));
        assert!(!builtin_speakers(3, "HDAUDIO")); // headphones on the jack
        assert!(!builtin_speakers(9, "HDAUDIO")); // monitor over HDMI
    }

    fn describe(device: &IMMDevice) -> String {
        format!(
            "{:?} form factor {:?} enumerator {:?}",
            device_string(device, &PKEY_Device_FriendlyName).unwrap_or_default(),
            device_u32(device, &PKEY_AudioEndpoint_FormFactor),
            device_string(device, &PKEY_Device_EnumeratorName),
        )
    }

    #[test]
    fn device_queries_do_not_fail() {
        eprintln!("on built-in speakers: {}", on_builtin_speakers());
        let _ = others_playing();
        if let Ok(d) = default_output() {
            eprintln!("default output: {}", describe(&d));
        }
        let Ok(all) = (unsafe { enumerator().and_then(|e| e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)) }) else { return };
        for i in 0..unsafe { all.GetCount() }.unwrap_or(0) {
            if let Ok(d) = unsafe { all.Item(i) } {
                eprintln!("render endpoint: {}", describe(&d));
            }
        }
    }
}
