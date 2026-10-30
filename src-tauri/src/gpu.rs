//! Whether Talaan uses the GPU, and remembering when it failed (Windows; the Mac uses Metal in process and only
//! reports it).
//!
//! On Windows all GPU work runs in helper processes (`talaan-llm-vulkan.exe`, `talaan-stt-vulkan.exe`) so that
//! talaan.exe never needs `vulkan-1.dll`. A helper is used only when:
//! - "Use the GPU" is on;
//! - System32 has a Vulkan loader (`vulkan-1.dll`, which GPU drivers install; a VM or Windows Sandbox has none, and
//!   starting a Vulkan build without it would pop up a missing-DLL dialog);
//! - the helper is installed next to talaan.exe (dev builds without the Vulkan SDK don't have it);
//! - it hasn't failed with this app version and this graphics driver (`gpu.json` in the data folder).
//!
//! A helper is started with implicit Vulkan layers off (Steam, Epic and other overlays hook every Vulkan app and are a
//! known cause of crashes). When it fails and the processor then does the same work, the failure is written to
//! `gpu.json`; updating the app or the driver, or Try again in Check setup, clears it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const LLM_HELPER: &str = "talaan-llm-vulkan";
pub const STT_HELPER: &str = crate::transcriber::helper_speech::GPU_HELPER;
const FILE: &str = "gpu.json";

/// Which GPU helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Local summary AI.
    Llm,
    /// Whisper.
    Stt,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::Llm => "llm",
            Kind::Stt => "stt",
        }
    }
    pub fn helper(self) -> &'static str {
        match self {
            Kind::Llm => LLM_HELPER,
            Kind::Stt => STT_HELPER,
        }
    }
}

/// Why the GPU isn't used (the part that isn't a failure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// "Use the GPU" is off.
    Off,
    /// No `vulkan-1.dll` in System32.
    NoVulkan,
    /// The helper isn't installed (a build without the Vulkan SDK).
    NoHelper,
    /// It failed before, with this app and driver.
    Failed(String),
}

/// What to run a job on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Gpu { exe: PathBuf, env: Vec<(String, String)> },
    Cpu(Skip),
}

/// gpu.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub app_version: String,
    /// `driver_fingerprint()` when the failures were seen.
    pub driver: String,
    /// "llm" / "stt" → what went wrong.
    #[serde(default)]
    pub failures: BTreeMap<String, Failure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    /// The error, technical and in English (the helper's own words), capped.
    pub reason: String,
    pub at: String,
}

/// Longest reason kept (the first lines of a crash report are what matter).
const REASON_BYTES: usize = 600;

/// One lock for every read-modify-write of gpu.json (the transcriber and the summarizer run on their own threads).
static WRITE: Mutex<()> = Mutex::new(());

/// The facts the decisions are made from, gathered once.
#[derive(Clone)]
pub struct Gpu {
    data_dir: PathBuf,
    app_version: String,
    driver: String,
    /// Where the helpers are (next to talaan.exe).
    helper_dir: PathBuf,
    loader: bool,
}

impl Gpu {
    /// This PC, now.
    pub fn detect(data_dir: &Path) -> Arc<Gpu> {
        let helper_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        Arc::new(Gpu { data_dir: data_dir.to_path_buf(), app_version: env!("CARGO_PKG_VERSION").into(), driver: driver_fingerprint(), helper_dir, loader: vulkan_loader_present() })
    }

    /// The app's one `Gpu` (detected on first use; the data folder never changes while Talaan runs).
    pub fn shared(data_dir: &Path) -> Arc<Gpu> {
        static SHARED: std::sync::OnceLock<Arc<Gpu>> = std::sync::OnceLock::new();
        SHARED.get_or_init(|| Gpu::detect(data_dir)).clone()
    }

    /// This PC, with the helpers in another folder (live tests: the test exe isn't next to them).
    #[cfg(test)]
    pub fn detect_with_helpers(data_dir: &Path, helper_dir: &Path) -> Gpu {
        Gpu { helper_dir: helper_dir.to_path_buf(), ..(*Gpu::detect(data_dir)).clone() }
    }

    #[cfg(test)]
    pub fn fake(data_dir: &Path, helper_dir: &Path, loader: bool, app_version: &str, driver: &str) -> Gpu {
        Gpu { data_dir: data_dir.into(), app_version: app_version.into(), driver: driver.into(), helper_dir: helper_dir.into(), loader }
    }

    pub fn helper_path(&self, kind: Kind) -> PathBuf {
        self.helper_dir.join(format!("{}{}", kind.helper(), std::env::consts::EXE_SUFFIX))
    }

    /// Whether to use the GPU helper for this kind of job. Never on the Mac (Metal is in process there).
    pub fn plan(&self, kind: Kind, use_gpu: bool) -> Plan {
        if !use_gpu {
            return Plan::Cpu(Skip::Off);
        }
        if !self.loader {
            return Plan::Cpu(Skip::NoVulkan);
        }
        let exe = self.helper_path(kind);
        if !exe.is_file() {
            return Plan::Cpu(Skip::NoHelper);
        }
        if let Some(f) = self.failure(kind) {
            return Plan::Cpu(Skip::Failed(f.reason));
        }
        Plan::Gpu { exe, env: helper_env() }
    }

    fn path(&self) -> PathBuf {
        self.data_dir.join(FILE)
    }

    /// The marker, if it was written for this app version and driver (an old one counts as nothing).
    pub fn marker(&self) -> Option<Marker> {
        let m: Marker = serde_json::from_slice(&std::fs::read(self.path()).ok()?).ok()?;
        (m.app_version == self.app_version && m.driver == self.driver).then_some(m)
    }

    pub fn failure(&self, kind: Kind) -> Option<Failure> {
        self.marker()?.failures.remove(kind.key())
    }

    /// Remembers that the GPU failed for this kind of job (the processor then did it).
    pub fn record(&self, kind: Kind, reason: &str) {
        log::warn!("the GPU failed for {} jobs; the processor is used until Try again or a driver update: {}", kind.helper(), crate::logging::text(reason));
        let _g = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let mut m = self.marker().unwrap_or_else(|| Marker { app_version: self.app_version.clone(), driver: self.driver.clone(), failures: BTreeMap::new() });
        let mut cut = reason.len().min(REASON_BYTES);
        while !reason.is_char_boundary(cut) {
            cut -= 1;
        }
        m.failures.insert(kind.key().into(), Failure { reason: reason[..cut].trim().to_string(), at: chrono::Local::now().fixed_offset().to_rfc3339() });
        let tmp = self.data_dir.join(format!("{FILE}.tmp"));
        let body = serde_json::to_vec_pretty(&m).expect("marker serializes");
        if std::fs::create_dir_all(&self.data_dir).is_ok() && std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, self.path());
        }
    }

    /// Try again: forget every failure.
    pub fn clear(&self) {
        let _g = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::fs::remove_file(self.path());
    }

    /// A callback that records failures of one kind (for the engines, which know nothing of gpu.json).
    pub fn recorder(self: &Arc<Self>, kind: Kind) -> Arc<dyn Fn(&str) + Send + Sync> {
        let gpu = self.clone();
        Arc::new(move |reason: &str| gpu.record(kind, reason))
    }
}

/// What a plan runs on, in a few words, for the log and the diagnostics.
pub fn describe(plan: &Plan) -> String {
    match plan {
        Plan::Gpu { exe, .. } => format!("GPU helper ({})", exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        Plan::Cpu(Skip::Off) => "processor (GPU turned off)".into(),
        Plan::Cpu(Skip::NoVulkan) => "processor (no Vulkan driver)".into(),
        Plan::Cpu(Skip::NoHelper) => "processor (no GPU helper in this build)".into(),
        Plan::Cpu(Skip::Failed(r)) => format!("processor (the GPU failed before: {})", r.lines().next().unwrap_or("").trim()),
    }
}

/// The environment a Vulkan helper starts with: implicit layers off. Anything set already (for example
/// `GGML_VK_VISIBLE_DEVICES` or `TALAAN_GPU=fail` for a test) is inherited as usual.
pub fn helper_env() -> Vec<(String, String)> {
    vec![("VK_LOADER_LAYERS_DISABLE".into(), "~implicit~".into())]
}

/// A stable description of the display drivers: adapter names and driver versions, sorted. It changes when a
/// driver is updated, which is when a remembered failure should be tried again.
pub fn fingerprint(mut adapters: Vec<(String, String)>) -> String {
    adapters.sort();
    adapters.dedup();
    adapters.iter().map(|(name, version)| format!("{} {}", name.trim(), version.trim())).collect::<Vec<_>>().join("; ")
}

#[cfg(windows)]
pub fn driver_fingerprint() -> String {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    // The display adapter device class.
    const CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(CLASS) else { return String::new() };
    let adapters = class
        .enum_keys()
        .flatten()
        // "0000", "0001"...; "Configuration" and "Properties" aren't adapters (and Properties can't be opened).
        .filter(|k| k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|k| {
            let key = class.open_subkey(&k).ok()?;
            Some((key.get_value::<String, _>("DriverDesc").ok()?, key.get_value::<String, _>("DriverVersion").unwrap_or_default()))
        })
        .collect();
    fingerprint(adapters)
}

#[cfg(not(windows))]
pub fn driver_fingerprint() -> String {
    String::new()
}

/// Whether `vulkan-1.dll` loads from System32. Only that folder is searched, so a stray copy elsewhere on the path
/// can't count. The library is freed right away; talaan.exe never imports or keeps it.
#[cfg(windows)]
pub fn vulkan_loader_present() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    match unsafe { LoadLibraryExW(w!("vulkan-1.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) } {
        Ok(h) => {
            let _ = unsafe { FreeLibrary(h) };
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn vulkan_loader_present() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(loader: bool, helpers: bool) -> (tempfile::TempDir, Gpu) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gpu = Gpu::fake(&dir.path().join("data"), &bin, loader, "1.1.0", "NVIDIA GeForce RTX 5070 Ti 32.0.15.9186");
        if helpers {
            for k in [Kind::Llm, Kind::Stt] {
                std::fs::write(gpu.helper_path(k), "").unwrap();
            }
        }
        (dir, gpu)
    }

    #[test]
    fn the_gpu_is_used_only_when_everything_is_there() {
        let (_d, gpu) = setup(true, true);
        let Plan::Gpu { exe, env } = gpu.plan(Kind::Llm, true) else { panic!() };
        assert!(exe.ends_with(format!("talaan-llm-vulkan{}", std::env::consts::EXE_SUFFIX)));
        assert!(env.contains(&("VK_LOADER_LAYERS_DISABLE".into(), "~implicit~".into())));
        assert!(matches!(gpu.plan(Kind::Stt, true), Plan::Gpu { .. }));
        assert_eq!(gpu.plan(Kind::Llm, false), Plan::Cpu(Skip::Off));
        let (_d, no_loader) = setup(false, true);
        assert_eq!(no_loader.plan(Kind::Stt, true), Plan::Cpu(Skip::NoVulkan));
        let (_d, no_helper) = setup(true, false);
        assert_eq!(no_helper.plan(Kind::Stt, true), Plan::Cpu(Skip::NoHelper));
    }

    #[test]
    fn a_failure_is_remembered_per_kind_until_cleared() {
        let (_d, gpu) = setup(true, true);
        assert_eq!(gpu.marker(), None);
        gpu.record(Kind::Stt, "speech GPU helper stopped: exit code -1073740791 (0xC0000409)\nggml_vulkan: Device lost");
        assert_eq!(gpu.plan(Kind::Stt, true), Plan::Cpu(Skip::Failed("speech GPU helper stopped: exit code -1073740791 (0xC0000409)\nggml_vulkan: Device lost".into())));
        assert!(matches!(gpu.plan(Kind::Llm, true), Plan::Gpu { .. }), "the summary helper isn't blamed");
        gpu.record(Kind::Llm, &"x".repeat(5000));
        assert_eq!(gpu.failure(Kind::Llm).unwrap().reason.len(), REASON_BYTES);
        assert!(gpu.failure(Kind::Stt).is_some(), "both kept");
        gpu.clear();
        assert!(matches!(gpu.plan(Kind::Stt, true), Plan::Gpu { .. }));
        gpu.clear(); // nothing to clear is fine
    }

    #[test]
    fn a_new_app_version_or_driver_tries_the_gpu_again() {
        let (d, gpu) = setup(true, true);
        gpu.record(Kind::Llm, "crashed");
        let bin = d.path().join("bin");
        let updated_app = Gpu::fake(&d.path().join("data"), &bin, true, "1.1.1", "NVIDIA GeForce RTX 5070 Ti 32.0.15.9186");
        assert!(matches!(updated_app.plan(Kind::Llm, true), Plan::Gpu { .. }));
        let updated_driver = Gpu::fake(&d.path().join("data"), &bin, true, "1.1.0", "NVIDIA GeForce RTX 5070 Ti 32.0.15.9999");
        assert!(matches!(updated_driver.plan(Kind::Llm, true), Plan::Gpu { .. }));
        // Recording under the new driver replaces the stale marker rather than adding to it.
        updated_driver.record(Kind::Stt, "again");
        let m = updated_driver.marker().unwrap();
        assert_eq!(m.failures.keys().collect::<Vec<_>>(), ["stt"]);
        assert!(gpu.failure(Kind::Llm).is_none(), "the old driver's entry is gone");
    }

    #[test]
    fn a_broken_marker_counts_as_none() {
        let (_d, gpu) = setup(true, true);
        std::fs::create_dir_all(&gpu.data_dir).unwrap();
        std::fs::write(gpu.path(), "{not json").unwrap();
        assert!(matches!(gpu.plan(Kind::Llm, true), Plan::Gpu { .. }));
        gpu.record(Kind::Llm, "x");
        assert!(gpu.failure(Kind::Llm).is_some());
    }

    #[test]
    fn the_driver_fingerprint_is_stable() {
        let a = fingerprint(vec![("NVIDIA GeForce RTX 5070 Ti".into(), "32.0.15.9186".into()), ("AMD Radeon(TM) Graphics".into(), "32.0.21025.1".into())]);
        let b = fingerprint(vec![("AMD Radeon(TM) Graphics".into(), "32.0.21025.1".into()), ("NVIDIA GeForce RTX 5070 Ti".into(), "32.0.15.9186".into())]);
        assert_eq!(a, b);
        assert_eq!(a, "AMD Radeon(TM) Graphics 32.0.21025.1; NVIDIA GeForce RTX 5070 Ti 32.0.15.9186");
        assert_ne!(a, fingerprint(vec![("NVIDIA GeForce RTX 5070 Ti".into(), "32.0.15.9999".into()), ("AMD Radeon(TM) Graphics".into(), "32.0.21025.1".into())]));
        assert_eq!(fingerprint(vec![]), "");
    }

    /// This PC: prints what Talaan sees (read-only). `cargo test gpu_here -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gpu_here() {
        eprintln!("vulkan-1.dll in System32: {}", vulkan_loader_present());
        eprintln!("driver: {}", driver_fingerprint());
    }
}
