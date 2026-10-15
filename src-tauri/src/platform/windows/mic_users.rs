//! Which apps are using the microphone right now, from two sources merged by `bundle_id`. No extra permission;
//! Talaan itself is left out of both.
//!
//! 1. **Capture sessions (live).** Every active WASAPI session on every active capture endpoint, mapped from its
//!    pid to the exe path and, for packaged apps, the package family name. This is what is capturing this
//!    second, for any kind of process. Talaan is excluded by pid.
//! 2. **Consent store (registry).** The per-app record Windows keeps under `CapabilityAccessManager\ConsentStore\
//!    microphone` (`LastUsedTimeStop` is 0 while an app holds the mic). Talaan is excluded by path / `talaan.exe`.
//!
//! Why both: a live check showed the consent store has NO entry for console or dev processes that capture (a
//! cargo test exe, python, a Tk app), so its 0 → non-zero stop transition was never seen for them (GUI apps do
//! appear, and show stop > start after use). It can't be trusted alone. Sessions are live and cover desktop and
//! packaged apps alike; the registry stays as a fallback for anything a session doesn't show (e.g. a process we
//! can't open). When both name an app the session entry wins, since it carries a real pid and path.
//!
//! A consent-store entry can stay at stop == 0 forever (an app killed while it held the mic). So when WASAPI
//! answered, a registry user only counts while a process with that exe name (desktop) or package family
//! (packaged) is running. When WASAPI couldn't be reached, the registry is used as it is.

use winreg::enums::{HKEY_CURRENT_USER, REG_BINARY, REG_QWORD};
use winreg::RegKey;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// A process capturing audio input. On Windows `bundle_id` is the packaged app's family name, or the desktop
/// app's exe file name in lower case; `path` is the desktop app's exe path (empty for packaged apps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: i32,
    pub bundle_id: String,
    pub path: String,
}

/// One consent-store subkey: its name and last-used times (FILETIME; 0 when absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub start: u64,
    /// `None` when the value is missing or unreadable: then we can't tell the app let go, so it isn't "in use".
    pub stop: Option<u64>,
}

fn in_use(e: &Entry) -> bool {
    e.start > 0 && e.stop == Some(0)
}

/// A FILETIME value: a REG_QWORD, or the same 8 bytes stored as REG_BINARY (little-endian). Anything else is `None`.
pub fn decode_time(raw: Option<&[u8]>) -> Option<u64> {
    let bytes: [u8; 8] = raw?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

fn read_time(key: &RegKey, name: &str) -> Option<u64> {
    let v = key.get_raw_value(name).ok()?;
    match v.vtype {
        REG_QWORD | REG_BINARY => decode_time(Some(&v.bytes)),
        _ => None,
    }
}

/// `C:#Program Files#Zoom#Zoom.exe` → (`C:\Program Files\Zoom\Zoom.exe`, `zoom.exe`).
/// A real `#` in a folder name is lossy (it also becomes `\`), which is harmless: the path is only used for matching.
pub fn desktop_user(key: &str) -> (String, String) {
    let path = key.replace('#', "\\");
    let exe = path.rsplit('\\').next().unwrap_or("").to_lowercase();
    (path, exe)
}

pub fn users_from(packaged: &[Entry], desktop: &[Entry], own_exe: &str) -> Vec<MicUser> {
    let packaged = packaged.iter().filter(|e| in_use(e)).map(|e| MicUser { pid: 0, bundle_id: e.key.clone(), path: String::new() });
    let desktop = desktop.iter().filter(|e| in_use(e)).filter_map(|e| {
        let (path, exe) = desktop_user(&e.key);
        let own = path.eq_ignore_ascii_case(own_exe) || exe == "talaan.exe";
        (!own).then_some(MicUser { pid: 0, bundle_id: exe, path })
    });
    packaged.chain(desktop).collect()
}

/// One audio session on a capture endpoint, as WASAPI reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub pid: u32,
    pub active: bool,
    /// The shared "System Sounds" session.
    pub system: bool,
}

/// The processes capturing right now: active, not the system-sounds session, not pid 0 (system) and not `own`.
/// Sorted, each pid once (a process can hold sessions on several mics).
pub fn live_pids(sessions: &[Session], own: Option<u32>) -> Vec<u32> {
    let mut pids: Vec<u32> = sessions.iter().filter(|s| s.active && !s.system && s.pid != 0 && Some(s.pid) != own).map(|s| s.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// The user behind a capturing pid: the package family name when the process is packaged, else its lowercased
/// exe file name. `None` when Windows wouldn't tell us anything about it (no path, no package).
pub fn session_user(pid: u32, path: &str, family: Option<&str>) -> Option<MicUser> {
    let bundle_id = match family.filter(|f| !f.is_empty()) {
        Some(f) => f.to_string(),
        None => path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase(),
    };
    (!bundle_id.is_empty()).then(|| MicUser { pid: pid as i32, bundle_id, path: path.to_string() })
}

/// Sessions first, then registry entries for apps no session already names (case-insensitive `bundle_id`).
/// Each `bundle_id` appears once; the session entry wins because it has a real pid and path.
pub fn merge(sessions: Vec<MicUser>, registry: Vec<MicUser>) -> Vec<MicUser> {
    let mut out: Vec<MicUser> = Vec::new();
    for user in sessions.into_iter().chain(registry) {
        if !out.iter().any(|o| o.bundle_id.eq_ignore_ascii_case(&user.bundle_id)) {
            out.push(user);
        }
    }
    out
}

/// The registry users whose app is still running: a desktop user (it has a path) needs a process with its exe
/// name, a packaged user (no path) a process in its package family. Both compared case-insensitively.
pub fn running_only(registry: Vec<MicUser>, exes: &[String], families: &[String]) -> Vec<MicUser> {
    let has = |list: &[String], name: &str| list.iter().any(|x| x.eq_ignore_ascii_case(name));
    registry.into_iter().filter(|u| if u.path.is_empty() { has(families, &u.bundle_id) } else { has(exes, &u.bundle_id) }).collect()
}

fn entries(key: &RegKey) -> Vec<Entry> {
    key.enum_keys()
        .filter_map(Result::ok)
        .filter(|name| name != "NonPackaged")
        .filter_map(|name| {
            let sub = key.open_subkey(&name).ok()?;
            let start = read_time(&sub, "LastUsedTimeStart").unwrap_or(0);
            let stop = read_time(&sub, "LastUsedTimeStop");
            Some(Entry { key: name, start, stop })
        })
        .collect()
}

/// Every app other than Talaan that has the microphone open right now: live capture sessions, plus the
/// consent store's "in use" apps no session already names.
pub fn mic_users() -> Vec<MicUser> {
    let scan = capture_sessions();
    let sessions = live_pids(scan.as_deref().unwrap_or(&[]), Some(std::process::id()))
        .into_iter()
        .filter_map(|pid| {
            let (path, family) = identity_of(pid);
            session_user(pid, &path, family.as_deref())
        })
        .collect();
    let mut registry = registry_users();
    // WASAPI answered, so the registry is only a fallback: drop entries whose app isn't running (stuck at stop == 0).
    if scan.is_some() && !registry.is_empty() {
        let running = running_processes();
        let exes: Vec<String> = running.iter().map(|(_, exe)| exe.clone()).collect();
        // Package families cost an OpenProcess per process, so only when a packaged entry needs them.
        let families: Vec<String> = if registry.iter().any(|u| u.path.is_empty()) { running.iter().filter_map(|(pid, _)| family_of(*pid)).collect() } else { vec![] };
        registry = running_only(registry, &exes, &families);
    }
    merge(sessions, registry)
}

/// (pid, exe file name) of every running process, from a Toolhelp snapshot. Empty if the snapshot fails.
fn running_processes() -> Vec<(u32, String)> {
    use ::windows::Win32::Foundation::CloseHandle;
    use ::windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};

    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let name = &entry.szExeFile;
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            out.push((entry.th32ProcessID, String::from_utf16_lossy(&name[..len])));
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// The package family name of a running process, `None` when it isn't packaged or can't be opened.
fn family_of(pid: u32) -> Option<String> {
    use ::windows::Win32::Foundation::CloseHandle;
    use ::windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let family = family_of_handle(handle);
        let _ = CloseHandle(handle);
        family
    }
}

unsafe fn family_of_handle(handle: ::windows::Win32::Foundation::HANDLE) -> Option<String> {
    use ::windows::core::PWSTR;
    use ::windows::Win32::Foundation::ERROR_SUCCESS;
    use ::windows::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
    let mut fam = [0u16; 256];
    let mut fam_len = fam.len() as u32;
    // Unpackaged processes return APPMODEL_ERROR_NO_PACKAGE; the length includes the trailing NUL.
    (unsafe { GetPackageFamilyName(handle, &mut fam_len, Some(PWSTR(fam.as_mut_ptr()))) } == ERROR_SUCCESS)
        .then(|| String::from_utf16_lossy(&fam[..(fam_len as usize).saturating_sub(1).min(fam.len())]))
}

/// The consent store's apps that hold the mic now, Talaan's own exe left out.
fn registry_users() -> Vec<MicUser> {
    let (packaged, desktop) = read_store();
    let own = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    users_from(&packaged, &desktop, &own)
}

/// Every audio session on every active capture endpoint, unfiltered. `None` if WASAPI can't be reached (no
/// device enumerator or endpoint list); a single device that fails is skipped.
pub fn capture_sessions() -> Option<Vec<Session>> {
    use ::windows::core::Interface;
    use ::windows::Win32::Media::Audio::{
        eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use ::windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

    super::com::init();
    let mut out = Vec::new();
    unsafe {
        let enumerator = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE).ok()?;
        for d in 0..devices.GetCount().unwrap_or(0) {
            let Ok(device) = devices.Item(d) else { continue };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
            for i in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(control) = sessions.GetSession(i).and_then(|c| c.cast::<IAudioSessionControl2>()) else { continue };
                let active = control.GetState().map(|s| s == AudioSessionStateActive).unwrap_or(false);
                // A session spanning several processes still reports one of them (AUDCLNT_S_NO_SINGLE_PROCESS is a success).
                let pid = control.GetProcessId().unwrap_or(0);
                let system = control.IsSystemSoundsSession() == ::windows::Win32::Foundation::S_OK;
                out.push(Session { pid, active, system });
            }
        }
    }
    Some(out)
}

/// (full exe path, package family name) of a running process; empty / `None` for whatever Windows won't say.
fn identity_of(pid: u32) -> (String, Option<String>) {
    use ::windows::core::PWSTR;
    use ::windows::Win32::Foundation::CloseHandle;
    use ::windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return (String::new(), None) };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let path = match QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) {
            Ok(()) => String::from_utf16_lossy(&buf[..len as usize]),
            Err(_) => String::new(),
        };
        let family = family_of_handle(handle);
        let _ = CloseHandle(handle);
        (path, family)
    }
}

/// (packaged entries, desktop entries) as the registry holds them right now.
fn read_store() -> (Vec<Entry>, Vec<Entry>) {
    let Ok(store) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(STORE) else { return (vec![], vec![]) };
    let desktop = store.open_subkey("NonPackaged").map(|k| entries(&k)).unwrap_or_default();
    (entries(&store), desktop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(key: &str, start: u64, stop: u64) -> Entry {
        Entry { key: key.into(), start, stop: Some(stop) }
    }

    #[test]
    fn filetime_values_decode_from_qword_or_8_byte_binary() {
        let t: u64 = 0x01DB_1234_5678_9ABC;
        assert_eq!(decode_time(Some(&t.to_le_bytes())), Some(t)); // REG_QWORD and 8-byte REG_BINARY look the same
        assert_eq!(decode_time(Some(&0u64.to_le_bytes())), Some(0));
        assert_eq!(decode_time(Some(&[1, 2, 3, 4])), None);
        assert_eq!(decode_time(Some(&[0; 9])), None);
        assert_eq!(decode_time(None), None);
    }

    #[test]
    fn a_start_with_no_readable_stop_is_not_in_use() {
        let desktop = [Entry { key: r"C:#x#a.exe".into(), start: 5, stop: None }];
        assert!(users_from(&[], &desktop, "").is_empty());
    }

    #[test]
    fn only_apps_holding_the_mic_now_are_users() {
        let packaged = [e("MSTeams_8wekyb3d8bbwe", 100, 0), e("91750D7E.Slack_8she8kybcnzg4", 100, 200), e("Clipchamp.Clipchamp_yxz26nhyzhsrt", 0, 0)];
        let desktop = [e(r"C:#Users#a#AppData#Roaming#Zoom#bin#Zoom.exe", 50, 0), e(r"C:#Program Files#Google#Chrome#Application#chrome.exe", 50, 60)];
        let users = users_from(&packaged, &desktop, r"C:\Program Files\Talaan\talaan.exe");
        assert_eq!(
            users,
            vec![
                MicUser { pid: 0, bundle_id: "MSTeams_8wekyb3d8bbwe".into(), path: String::new() },
                MicUser { pid: 0, bundle_id: "zoom.exe".into(), path: r"C:\Users\a\AppData\Roaming\Zoom\bin\Zoom.exe".into() },
            ]
        );
    }

    #[test]
    fn talaan_itself_is_left_out() {
        let desktop = [e(r"C:#Program Files#Talaan#Talaan.exe", 1, 0), e(r"C:#Dev#Talaan#src-tauri#target#debug#talaan.exe", 1, 0)];
        assert!(users_from(&[], &desktop, r"C:\Program Files\Talaan\talaan.exe").is_empty());
    }

    #[test]
    fn desktop_keys_become_paths_and_lowercase_exe_names() {
        assert_eq!(desktop_user(r"D:#SteamLibrary#x#Discord.EXE"), (r"D:\SteamLibrary\x\Discord.EXE".to_string(), "discord.exe".to_string()));
    }

    fn s(pid: u32, active: bool, system: bool) -> Session {
        Session { pid, active, system }
    }

    fn u(pid: i32, bundle_id: &str, path: &str) -> MicUser {
        MicUser { pid, bundle_id: bundle_id.into(), path: path.into() }
    }

    #[test]
    fn only_active_non_system_sessions_of_other_processes_count() {
        let sessions = [s(40, true, false), s(0, true, false), s(7, true, true), s(12, false, false), s(99, true, false), s(40, true, false), s(5, true, false)];
        assert_eq!(live_pids(&sessions, Some(99)), vec![5, 40]);
        assert_eq!(live_pids(&sessions, None), vec![5, 40, 99]);
        assert!(live_pids(&[], Some(1)).is_empty());
    }

    #[test]
    fn a_desktop_process_is_its_lowercased_exe_name() {
        assert_eq!(session_user(42, r"C:\Users\a\AppData\Roaming\Zoom\bin\Zoom.exe", None), Some(u(42, "zoom.exe", r"C:\Users\a\AppData\Roaming\Zoom\bin\Zoom.exe")));
    }

    #[test]
    fn a_packaged_process_is_its_family_name() {
        let path = r"C:\Program Files\WindowsApps\MSTeams_25.1.0.0_x64__8wekyb3d8bbwe\ms-teams.exe";
        assert_eq!(session_user(7, path, Some("MSTeams_8wekyb3d8bbwe")), Some(u(7, "MSTeams_8wekyb3d8bbwe", path)));
        assert_eq!(session_user(7, "", Some("MSTeams_8wekyb3d8bbwe")), Some(u(7, "MSTeams_8wekyb3d8bbwe", "")));
        assert_eq!(session_user(7, path, Some("")), Some(u(7, "ms-teams.exe", path)), "an empty family is no family");
    }

    #[test]
    fn a_process_windows_says_nothing_about_is_dropped() {
        assert_eq!(session_user(8, "", None), None);
        assert_eq!(session_user(8, "", Some("")), None);
    }

    #[test]
    fn sessions_win_over_registry_entries_for_the_same_app() {
        let sessions = vec![u(42, "zoom.exe", r"C:\z\Zoom.exe"), u(43, "zoom.exe", r"C:\z\Zoom.exe"), u(7, "MSTeams_8wekyb3d8bbwe", "")];
        let registry = vec![u(0, "zoom.exe", r"C:\z\Zoom.exe"), u(0, "msteams_8wekyb3d8bbwe", ""), u(0, "discord.exe", r"C:\d\Discord.exe")];
        assert_eq!(merge(sessions, registry), vec![u(42, "zoom.exe", r"C:\z\Zoom.exe"), u(7, "MSTeams_8wekyb3d8bbwe", ""), u(0, "discord.exe", r"C:\d\Discord.exe")]);
        assert_eq!(merge(vec![], vec![u(0, "a.exe", "")]), vec![u(0, "a.exe", "")]);
        assert_eq!(merge(vec![u(1, "a.exe", "")], vec![]), vec![u(1, "a.exe", "")]);
    }

    #[test]
    fn registry_users_count_only_while_their_app_runs() {
        let registry = vec![
            u(0, "zoom.exe", r"C:\z\Zoom.exe"),      // killed while holding the mic: stop stuck at 0
            u(0, "discord.exe", r"C:\d\Discord.exe"), // running
            u(0, "MSTeams_8wekyb3d8bbwe", ""),        // packaged, running
            u(0, "91750D7E.Slack_8she8kybcnzg4", ""), // packaged, gone
        ];
        let exes = vec!["Discord.exe".to_string(), "ms-teams.exe".to_string(), "explorer.exe".to_string()];
        let families = vec!["msteams_8wekyb3d8bbwe".to_string()];
        assert_eq!(running_only(registry.clone(), &exes, &families), vec![u(0, "discord.exe", r"C:\d\Discord.exe"), u(0, "MSTeams_8wekyb3d8bbwe", "")]);
        assert!(running_only(registry, &[], &[]).is_empty());
        // A desktop exe name never stands in for a package family, nor the other way round.
        assert!(running_only(vec![u(0, "slack.exe", r"C:\s\slack.exe")], &[], &["slack.exe".into()]).is_empty());
        assert!(running_only(vec![u(0, "MSTeams_8wekyb3d8bbwe", "")], &["MSTeams_8wekyb3d8bbwe".into()], &[]).is_empty());
    }

    #[test]
    fn reading_the_real_registry_does_not_fail() {
        let t = std::time::Instant::now();
        let users = mic_users();
        eprintln!("mic_users() took {:?}: {users:#?}", t.elapsed());
        eprintln!("registry only: {:#?}", registry_users());
        let t = std::time::Instant::now();
        let raw = capture_sessions().expect("WASAPI reachable");
        eprintln!("capture_sessions() took {:?}", t.elapsed());
        let all: Vec<_> = raw.iter().map(|s| (s, identity_of(s.pid))).collect();
        eprintln!("all capture sessions (active or not) with (path, family): {all:#?}");
        // The extra per-tick cost when the registry has users: the snapshot always, families only for packaged ones.
        let t = std::time::Instant::now();
        let running = running_processes();
        let snap = t.elapsed();
        let t = std::time::Instant::now();
        let families: Vec<String> = running.iter().filter_map(|(pid, _)| family_of(*pid)).collect();
        eprintln!("snapshot of {} processes took {snap:?}; package families ({} packaged) took {:?}", running.len(), families.len(), t.elapsed());
        assert!(!running.is_empty());
    }

    /// Records from the real mic and checks that our own pid shows up as an active capture session while it
    /// does, is kept out of `mic_users()`, and is gone once the mic stops. Run by hand:
    /// `cargo test capture_sessions_live_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn capture_sessions_live_smoke() {
        use crate::recorder::feed::feed;
        use crate::recorder::mic::MicSource;
        use crate::recorder::source::AudioSource;
        let own = std::process::id();
        let active = || live_pids(&capture_sessions().expect("WASAPI reachable"), None);

        eprintln!("own pid {own}; active before: {:?}", active());
        let (producer, _consumer) = feed();
        let mut mic = MicSource::default();
        eprintln!("recording from {}", mic.start(std::sync::Arc::new(std::sync::Mutex::new(producer))).unwrap());
        std::thread::sleep(std::time::Duration::from_secs(1));

        let t = std::time::Instant::now();
        let during = active();
        eprintln!("active while recording ({:?}): {during:?}", t.elapsed());
        assert!(during.contains(&own), "our pid {own} should be an active capture session");
        let t = std::time::Instant::now();
        let users = mic_users();
        eprintln!("mic_users() while recording ({:?}): {users:#?}", t.elapsed());
        assert!(users.iter().all(|u| u.pid as u32 != own), "Talaan's own pid must be excluded");
        let exe = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(users.iter().all(|u| u.bundle_id != exe), "Talaan's own exe must be excluded");

        mic.stop();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let after = active();
        eprintln!("active after stopping: {after:?}");
        assert!(!after.contains(&own), "our pid {own} should no longer be an active session");
    }

    /// Records from the real mic and prints this test exe's consent-store entry while recording and after.
    /// Run by hand: `cargo test mic_users_live_smoke -- --ignored --nocapture`
    /// Finding on the dev PC (2026-10-15): Windows writes NO consent-store entry for this test exe (nor for a
    /// console python.exe) even while cpal receives mic buffers, so the entry prints as None. Real GUI apps
    /// (parla.exe, bf6.exe, ...) do appear, with stop > start after use.
    #[test]
    #[ignore]
    fn mic_users_live_smoke() {
        use crate::recorder::feed::feed;
        use crate::recorder::mic::MicSource;
        use crate::recorder::source::AudioSource;
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let mine = || read_store().1.into_iter().find(|e| desktop_user(&e.key).0.eq_ignore_ascii_case(&exe));

        let (producer, mut consumer) = feed();
        let mut mic = MicSource::default();
        mic.start(std::sync::Arc::new(std::sync::Mutex::new(producer))).unwrap();
        let waited = std::time::Instant::now();
        while mine().is_none() && waited.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        eprintln!("entry appeared after {:?}", waited.elapsed());
        let (packaged, desktop) = read_store();
        let mut buf = vec![];
        let mut n = 0;
        while consumer.pop(&mut buf).is_some() {
            n += 1;
        }
        eprintln!("mic buffers received: {n}");
        eprintln!("exe = {exe}");
        let mut newest: Vec<_> = desktop.iter().chain(packaged.iter()).collect();
        newest.sort_by_key(|e| std::cmp::Reverse(e.start));
        eprintln!("newest entries: {:#?}", &newest[..newest.len().min(4)]);
        eprintln!("WHILE RECORDING: {:?}", mine());
        eprintln!("mic_users() while recording (we are excluded): {:?}", mic_users());
        eprintln!("users_from with no own exe: {:?}", users_from(&packaged, &desktop, ""));
        assert!(mic_users().is_empty(), "Talaan's own exe must be excluded");
        let during = mine();
        eprintln!("DURING: {during:?}");

        mic.stop();
        std::thread::sleep(std::time::Duration::from_secs(3));
        let after = mine();
        eprintln!("AFTER STOPPING:  {after:?}");
    }
}
