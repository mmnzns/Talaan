//! Which apps are using the microphone right now: the per-app consent store Windows keeps in the registry
//! (`LastUsedTimeStop` is 0 while an app holds the mic). No extra permission. Talaan itself is left out.

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

/// Every app other than Talaan that has the microphone open right now.
pub fn mic_users() -> Vec<MicUser> {
    let (packaged, desktop) = read_store();
    let own = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    users_from(&packaged, &desktop, &own)
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

    #[test]
    fn reading_the_real_registry_does_not_fail() {
        eprintln!("{:#?}", mic_users());
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
