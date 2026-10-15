//! Default device changes (headphones in or out) and whether sound is coming out of the computer's own speakers.

use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_EnumeratorName;
use ::windows::Win32::Foundation::PROPERTYKEY;
use ::windows::core::{implement, PCWSTR};
use ::windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    DEVICE_STATE, PKEY_AudioEndpoint_FormFactor,
};
use std::sync::Arc;
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

/// Windows announces each default change once per role. Only the console role (what apps play to) matters;
/// communications and multimedia fire alongside it and would restart the recording twice.
pub fn event_for(flow: EDataFlow, role: ERole) -> Option<DeviceEvent> {
    match (flow, role) {
        (f, r) if f == eRender && r == eConsole => Some(DeviceEvent::OutputChanged),
        (f, r) if f == eCapture && r == eConsole => Some(DeviceEvent::InputChanged),
        _ => None,
    }
}

type OnChange = Arc<dyn Fn(DeviceEvent) + Send + Sync>;

#[implement(IMMNotificationClient)]
struct Client(OnChange);

impl IMMNotificationClient_Impl for Client_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> ::windows::core::Result<()> {
        if let Some(e) = event_for(flow, role) {
            (self.0)(e);
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> ::windows::core::Result<()> {
        Ok(())
    }
}

/// Listens for default input/output changes until dropped. The callback runs on a Windows audio thread, so it
/// must return quickly.
pub struct DeviceWatcher {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

// COM pointers from the multithreaded apartment; only used to unregister on drop.
unsafe impl Send for DeviceWatcher {}

impl DeviceWatcher {
    pub fn new(on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        let enumerator = enumerator().map_err(|e| e.to_string())?;
        let client: IMMNotificationClient = Client(Arc::new(on_change)).into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }.map_err(|e| e.to_string())?;
        Ok(DeviceWatcher { enumerator, client })
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = self.enumerator.UnregisterEndpointNotificationCallback(&self.client);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use ::windows::Win32::Media::Audio::{eCommunications, eMultimedia, DEVICE_STATE_ACTIVE};

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        let w = DeviceWatcher::new(|_| {}).unwrap();
        drop(w);
    }

    /// Manual: switch output or input in the taskbar flyout while this runs; each switch prints one event.
    #[test]
    #[ignore]
    fn print_device_events_for_20s() {
        let w = DeviceWatcher::new(|e| eprintln!("device event: {e:?}")).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(20));
        drop(w);
        eprintln!("watcher removed");
    }

    #[test]
    fn only_console_default_changes_matter() {
        assert_eq!(event_for(eRender, eConsole), Some(DeviceEvent::OutputChanged));
        assert_eq!(event_for(eCapture, eConsole), Some(DeviceEvent::InputChanged));
        assert_eq!(event_for(eRender, eCommunications), None); // fires alongside eConsole; avoid double restarts
        assert_eq!(event_for(eRender, eMultimedia), None);
    }

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
