//! Default device changes (headphones in or out) and whether sound is coming out of the computer's own speakers.

use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_EnumeratorName;
use ::windows::Win32::Foundation::PROPERTYKEY;
use ::windows::core::{implement, PCWSTR};
use ::windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    DEVICE_STATE, DEVICE_STATE_ACTIVE, PKEY_AudioEndpoint_FormFactor,
};
use ::windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use ::windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc, PropVariantToUInt32, PROPVARIANT};
use ::windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

const FORM_FACTOR_SPEAKERS: u32 = 1;

/// Speakers wired to this computer (built in, or on its audio jack), not USB or Bluetooth.
/// HDMI/DisplayPort monitors report form factor 9, headphones 3, headsets 5, so they never match.
/// A missing enumerator name ("") with form factor Speakers counts as built in: warning wrongly beats missing it.
pub fn builtin_speakers(form_factor: u32, enumerator: &str) -> bool {
    let external = ["USB", "BTHENUM", "BTHHFENUM", "BTHLEDEVICE"].iter().any(|e| enumerator.eq_ignore_ascii_case(e));
    form_factor == FORM_FACTOR_SPEAKERS && !external
}

/// A software output (FxSound, Voicemeeter, Sonar) that relays to a real device underneath. Only "ROOT" for now:
/// "SWD" is also used by some real hardware.
fn is_virtual(enumerator: &str) -> bool {
    enumerator.eq_ignore_ascii_case("ROOT")
}

/// `default` and each endpoint are (form factor, enumerator name); endpoints also say whether they're playing.
/// A real default output is judged on its own. A virtual one hides the real output, so look underneath: some wired
/// speaker endpoint (not itself virtual) has to be playing.
pub fn speakers_rule(default: (u32, &str), endpoints: &[(u32, &str, bool)]) -> bool {
    if !is_virtual(default.1) {
        return builtin_speakers(default.0, default.1);
    }
    endpoints.iter().any(|&(form, enumerator, playing)| playing && !is_virtual(enumerator) && builtin_speakers(form, enumerator))
}

/// How long wired speakers count as playing after they were last heard, so a pause in the call doesn't flicker
/// the warning.
const PLAYING_LATCH_NS: u64 = 8_000_000_000;

/// Quieter than this on the endpoint's meter is silence.
const PEAK_SILENCE: f32 = 1e-4;

pub fn heard_recently(last_heard_ns: u64, now_ns: u64) -> bool {
    last_heard_ns != 0 && now_ns.saturating_sub(last_heard_ns) < PLAYING_LATCH_NS
}

/// When wired speakers were last heard under a virtual default output (host ns; 0 = never).
static SPEAKERS_LAST_HEARD: AtomicU64 = AtomicU64::new(0);

/// True when call audio is coming out of speakers the mic can hear: speakers wired to this computer, either as the
/// default output or, when the default is a virtual router such as FxSound, playing underneath it. With FxSound in
/// front of a USB headset this is false.
pub fn on_builtin_speakers() -> bool {
    let Ok(device) = default_output() else { return false };
    let Some((form, enumerator)) = form_and_enumerator(&device) else { return false };
    if !is_virtual(&enumerator) {
        return speakers_rule((form, &enumerator), &[]);
    }
    let now = super::clock::host_now_ns();
    let mut endpoints = vec![];
    for d in active_outputs() {
        let Some((f, e)) = form_and_enumerator(&d) else { continue };
        let wired = !is_virtual(&e) && builtin_speakers(f, &e);
        if wired && peak(&d) > PEAK_SILENCE {
            SPEAKERS_LAST_HEARD.store(now, Ordering::Relaxed);
        }
        endpoints.push((f, e, wired));
    }
    let playing = heard_recently(SPEAKERS_LAST_HEARD.load(Ordering::Relaxed), now);
    let endpoints: Vec<_> = endpoints.iter().map(|(f, e, wired)| (*f, e.as_str(), *wired && playing)).collect();
    speakers_rule((form, &enumerator), &endpoints)
}

fn form_and_enumerator(device: &IMMDevice) -> Option<(u32, String)> {
    Some((device_u32(device, &PKEY_AudioEndpoint_FormFactor).ok()?, device_string(device, &PKEY_Device_EnumeratorName).ok()?))
}

fn active_outputs() -> Vec<IMMDevice> {
    let Ok(all) = (unsafe { enumerator().and_then(|e| e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)) }) else { return vec![] };
    (0..unsafe { all.GetCount() }.unwrap_or(0)).filter_map(|i| unsafe { all.Item(i) }.ok()).collect()
}

/// The endpoint's current output peak (0.0–1.0); 0 if the meter can't be read.
fn peak(device: &IMMDevice) -> f32 {
    unsafe { device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None).and_then(|m| m.GetPeakValue()) }.unwrap_or(0.0)
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

type OnChange = Box<dyn Fn(DeviceEvent) + Send + Sync>;

#[implement(IMMNotificationClient)]
struct Client(OnChange);

impl IMMNotificationClient_Impl for Client_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> ::windows::core::Result<()> {
        if let Some(e) = event_for(flow, role) {
            // A panic must not unwind into Windows: that aborts the process, and the recording with it.
            // (Release builds use panic = "abort", so this guard only helps in debug builds and tests.)
            let _ = catch_unwind(AssertUnwindSafe(|| (self.0)(e)));
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

// Usually created on tao's main thread, which is a single-threaded apartment (com::init's MTA call fails there with
// RPC_E_CHANGED_MODE and the thread keeps its STA). Moving it is still sound: MMDeviceEnumerator is
// ThreadingModel=Both, so its pointer isn't apartment-bound, and the watcher only uses it to unregister.
unsafe impl Send for DeviceWatcher {}

impl DeviceWatcher {
    pub fn new(on_change: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, String> {
        let enumerator = enumerator().map_err(|e| e.to_string())?;
        let client: IMMNotificationClient = Client(Box::new(on_change)).into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }.map_err(|e| e.to_string())?;
        Ok(DeviceWatcher { enumerator, client })
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = self.enumerator.UnregisterEndpointNotificationCallback(&self.client);
        }
        // Unregister isn't documented to wait for a callback already running on an audio thread. Leak one
        // reference so the client (and the closure it holds) is never freed under that callback: one small
        // allocation per watcher, and sound whatever Unregister does.
        std::mem::forget(self.client.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use ::windows::Win32::Media::Audio::{eCommunications, eMultimedia};

    #[test]
    fn a_watcher_can_be_added_and_removed() {
        // A CI runner may have no audio service; then there is nothing to watch.
        match DeviceWatcher::new(|_| {}) {
            Ok(w) => drop(w),
            Err(e) => eprintln!("device watcher unavailable: {e:?}"),
        }
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

    #[test]
    fn a_physical_default_is_judged_on_its_own() {
        assert!(speakers_rule((1, "HDAUDIO"), &[]));
        assert!(!speakers_rule((1, "USB"), &[]));
        assert!(!speakers_rule((3, "HDAUDIO"), &[(1, "HDAUDIO", true)])); // headphones: other endpoints don't matter
        assert!(speakers_rule((1, "SWD"), &[])); // SWD isn't treated as virtual: some real hardware uses it
    }

    #[test]
    fn a_virtual_default_looks_for_wired_speakers_playing() {
        let fxsound = (1, "ROOT");
        // FxSound itself playing, wired speakers silent: the sound goes to the headset behind FxSound.
        assert!(!speakers_rule(fxsound, &[(1, "ROOT", true), (1, "HDAUDIO", false), (1, "USB", true)]));
        assert!(speakers_rule(fxsound, &[(1, "ROOT", true), (1, "HDAUDIO", true)]));
        assert!(!speakers_rule(fxsound, &[(1, "USB", true)])); // USB speakers playing: not this computer's
        assert!(!speakers_rule(fxsound, &[(9, "HDAUDIO", true)])); // monitor over HDMI
        assert!(!speakers_rule(fxsound, &[]));
    }

    #[test]
    fn speakers_count_as_playing_for_8s_after_last_heard() {
        let s = 1_000_000_000;
        assert!(!heard_recently(0, 100 * s)); // never heard
        assert!(heard_recently(100 * s, 100 * s));
        assert!(heard_recently(100 * s, 107 * s));
        assert!(!heard_recently(100 * s, 108 * s));
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
        for d in active_outputs() {
            eprintln!("render endpoint: {} peak {}", describe(&d), peak(&d));
        }
    }
}
