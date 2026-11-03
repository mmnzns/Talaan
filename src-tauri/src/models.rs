//! The models Talaan uses, kept in the shared folder by voice-engine ("Craft Concepts Digital/Models"), next to
//! Ensayo's and Parlato's. Talaan never deletes a model file itself: Delete is `release`, and the engine erases the
//! file once no app uses it.

use std::path::{Path, PathBuf};

use serde_json::Value;
use voice_engine_models::{catalog, App, LegacySource, ModelHandle, ModelInfo, SharedStore, Status, StoreError};

/// Talaan's recommended speech model.
pub const DEFAULT_SPEECH: &str = "whisper-turbo";
/// Talaan's recommended local AI model (long meetings); `qwen3.5-9b` is the optional larger one.
pub const DEFAULT_LOCAL_AI: &str = "qwen3.5-4b";
pub const VAD: &str = "vad";
pub const DIARIZATION: &str = "diarization";
pub const SEARCH: &str = "search";

pub struct Models {
    store: SharedStore,
    /// Talaan's old model folders (the engine's `move_in` leaves a hard link of each moved file there, so 1.1.1 keeps
    /// working). Empty on a `TALAAN_DATA_DIR` run: a test never touches the real folders.
    legacy: Vec<PathBuf>,
}

/// Talaan's old model folders on this computer: the Mac's `Application Support/Talaan/models`, Windows'
/// `%LOCALAPPDATA%\<bundle id>\models` and the older `%APPDATA%\Talaan\models`. The one place that says so.
pub fn legacy_dirs() -> Vec<PathBuf> {
    match dirs::data_local_dir() {
        Some(local) => legacy_dirs_in(&local, dirs::data_dir().as_deref(), cfg!(target_os = "windows")),
        None => Vec::new(),
    }
}

/// `legacy_dirs` for given folders (`local` is `%LOCALAPPDATA%` / `~/Library/Application Support`, `roaming` is
/// `%APPDATA%`): the engine's list for Talaan, so the two can't drift apart.
pub fn legacy_dirs_in(local: &Path, roaming: Option<&Path>, windows: bool) -> Vec<PathBuf> {
    LegacySource::all(App::Talaan, local, roaming, windows).into_iter().map(|s| s.root).collect()
}

impl Models {
    /// The shared folder, or `<test data>/shared-models` when `TALAAN_DATA_DIR` points Talaan at a test folder.
    /// `open_at` never moves anything in, so a test run can't touch the real models.
    pub fn open(test_data: Option<&Path>) -> std::io::Result<Models> {
        let (store, legacy) = match test_data {
            Some(dir) => (SharedStore::open_at(App::Talaan, dir.join("shared-models"))?, Vec::new()),
            None => (SharedStore::open(App::Talaan)?, legacy_dirs()),
        };
        Ok(Models { store, legacy })
    }

    /// Tests: the same models with these as Talaan's old folders.
    #[cfg(test)]
    pub fn with_legacy(mut self, dirs: Vec<PathBuf>) -> Models {
        self.legacy = dirs;
        self
    }

    /// Talaan's old model folders (empty on a test run).
    pub fn legacy_dirs(&self) -> &[PathBuf] {
        &self.legacy
    }

    /// Removes the hard links the move left in Talaan's old folders for one model, so Delete really frees the space.
    /// Only `<old folder>/<id>` goes; the engine's own copy is `release`'s business.
    pub fn remove_legacy_copy(&self, id: &str) {
        // An id from the catalog only (never a path from outside).
        let Some(model) = catalog::find(id) else { return };
        for dir in &self.legacy {
            let folder = dir.join(model.id);
            if folder.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&folder) {
                    let kind = e.kind();
                    log::warn!("couldn't remove Talaan's old copy of a model ({kind})");
                }
            }
        }
    }

    /// "Delete all data": Talaan's old model folders go whole (1.1.1 removed them too). On the Mac this one is
    /// inside the data folder and is already gone with it; on Windows `%LOCALAPPDATA%\<bundle id>\models` is not.
    pub fn remove_legacy_dirs(&self) {
        for dir in &self.legacy {
            if let Err(e) = std::fs::remove_dir_all(dir) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    let kind = e.kind();
                    log::warn!("couldn't remove Talaan's old models folder ({kind})");
                }
            }
        }
    }

    /// First thing at start, before any model is opened or helper started: the one-time move from Talaan's old
    /// folders, then the engine's clean-up of models no app uses.
    pub fn start(&self, settings: Option<&Value>) {
        match self.store.move_in(&used_ids(settings)) {
            Ok(r) if !r.already_done => log::info!(
                "models moved into the shared folder: {} moved, {} linked, {} left for later",
                r.moved.len(),
                r.linked.len(),
                r.left_for_later.len()
            ),
            Ok(_) => {}
            Err(e) => {
                let kind = e.kind();
                log::warn!("couldn't move the models into the shared folder ({kind})");
            }
        }
        // `move_in` records the users only the first time; a model another app installed since is Talaan's to
        // use too, so say so on every start (before `collect`, which erases models nobody uses).
        self.claim(settings);
        if let Err(e) = self.store.collect() {
            let kind = e.kind();
            log::warn!("couldn't tidy the shared models folder ({kind})");
        }
    }

    /// Records Talaan as a user of each model its settings point to that is already on this computer, so that if
    /// another app (Ensayo, Parlato) deletes one, the file stays. A model not installed yet is claimed when its
    /// download finishes. Run at start and after every settings save.
    ///
    /// No settings file (a fresh install, or after Delete all data) claims nothing: the defaults would otherwise
    /// make Talaan a user again of whatever another app's models are on this computer.
    pub fn claim(&self, settings: Option<&Value>) {
        let Some(settings) = settings else { return };
        for id in used_ids(Some(settings)) {
            if !matches!(self.store.status(id), Status::Installed { .. }) {
                continue;
            }
            if self.store.use_model(id).is_err() {
                // The error text can hold a path (the user's name): a fixed message only.
                log::warn!("couldn't record that Talaan uses an installed model");
            }
        }
    }

    pub fn root(&self) -> &Path {
        self.store.root()
    }

    pub fn store(&self) -> &SharedStore {
        &self.store
    }

    pub fn info(&self, id: &str) -> Option<&'static ModelInfo> {
        catalog::find(id)
    }

    pub fn status(&self, id: &str) -> Status {
        self.store.status(id)
    }

    /// The model's folder when every file is there.
    pub fn ready_dir(&self, id: &str) -> Option<PathBuf> {
        match self.store.status(id) {
            Status::Installed { dir, .. } => Some(dir),
            _ => None,
        }
    }

    /// Keep the returned handle for as long as the model is loaded.
    pub fn hold(&self, id: &str) -> Result<ModelHandle, StoreError> {
        self.store.hold(id)
    }
}

/// The model ids Talaan's settings point to. `move_in` records Talaan as a user of each.
/// Same reading as the worker (an unknown or non-speech `activeSpeechModel` is the default) and the summarizer
/// (any `summaryEngine` other than "ollama" or "byok" is the local model).
pub fn used_ids(settings: Option<&Value>) -> Vec<&'static str> {
    let s = settings.cloned().unwrap_or(Value::Null);
    let speech = s["activeSpeechModel"]
        .as_str()
        .and_then(catalog::find)
        .filter(|m| m.is_speech())
        .map_or(DEFAULT_SPEECH, |m| m.id);
    let mut ids = vec![speech, VAD];
    if s["separateSpeakers"].as_bool().unwrap_or(true) {
        ids.push(DIARIZATION);
    }
    if !matches!(s["summaryEngine"].as_str(), Some("ollama" | "byok")) {
        ids.push(local_model_id(s["localModel"].as_str()));
    }
    ids
}

/// The `localModel` setting as a catalog model.
pub fn local_model(setting: Option<&str>) -> &'static ModelInfo {
    catalog::find(local_model_id(setting)).expect("local_model_id returns a catalog id")
}

/// The `localModel` setting as a catalog id. 1.1 and earlier stored "recommended" or "larger".
pub fn local_model_id(setting: Option<&str>) -> &'static str {
    match setting {
        Some("larger") => "qwen3.5-9b",
        Some(id) => catalog::local_ai().find(|m| m.id == id).map_or(DEFAULT_LOCAL_AI, |m| m.id),
        None => DEFAULT_LOCAL_AI,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_settings_say_which_models_talaan_uses() {
        let s = json!({"activeSpeechModel": "parakeet-ultra", "separateSpeakers": true,
                       "summaryEngine": "local", "localModel": "larger"});
        assert_eq!(used_ids(Some(&s)), vec!["parakeet-ultra", "vad", "diarization", "qwen3.5-9b"]);
        let s = json!({"separateSpeakers": false, "summaryEngine": "byok"});
        assert_eq!(used_ids(Some(&s)), vec!["whisper-turbo", "vad"]);
        assert_eq!(used_ids(None), vec!["whisper-turbo", "vad", "diarization", "qwen3.5-4b"]);
    }

    #[test]
    fn an_unknown_summary_engine_counts_as_local_like_the_summarizer() {
        let s = json!({"separateSpeakers": false, "summaryEngine": "something-new"});
        assert_eq!(used_ids(Some(&s)), vec!["whisper-turbo", "vad", "qwen3.5-4b"]);
        let s = json!({"separateSpeakers": false, "summaryEngine": "ollama"});
        assert_eq!(used_ids(Some(&s)), vec!["whisper-turbo", "vad"]);
    }

    #[test]
    fn an_unknown_or_non_speech_speech_model_is_the_default() {
        for bad in ["nope", "vad", "diarization"] {
            let s = json!({"activeSpeechModel": bad, "separateSpeakers": false, "summaryEngine": "byok"});
            assert_eq!(used_ids(Some(&s)), vec!["whisper-turbo", "vad"], "{bad}");
        }
    }

    #[test]
    fn an_old_local_model_setting_becomes_a_model_id() {
        assert_eq!(local_model_id(Some("recommended")), "qwen3.5-4b");
        assert_eq!(local_model_id(Some("larger")), "qwen3.5-9b");
        assert_eq!(local_model_id(Some("granite-4.2-3b")), "granite-4.2-3b");
        assert_eq!(local_model_id(Some("whisper-turbo")), "qwen3.5-4b"); // not a local AI model
        assert_eq!(local_model_id(None), "qwen3.5-4b");
    }

    #[test]
    fn a_test_folder_never_reaches_the_real_shared_folder() {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        assert!(models.root().starts_with(data.path()));
        assert_eq!(models.root(), data.path().join("shared-models"));
    }

    #[test]
    fn a_model_is_ready_only_when_complete() {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        assert!(models.ready_dir("whisper-turbo").is_none());
        assert!(models.info("nope").is_none());
        assert_eq!(models.info("vad").unwrap().id, "vad");
    }

    /// Writes every file of a model at its exact catalog size (sparse), as if another app had installed it.
    fn install_without_using(models: &Models, id: &str) {
        for file in catalog::find(id).unwrap().files {
            let path = models.root().join(id).join(file.name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(path).unwrap().set_len(file.size).unwrap();
        }
    }

    #[test]
    fn claiming_records_talaan_as_a_user_of_the_installed_models_its_settings_point_to() {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        install_without_using(&models, "parakeet-ultra");
        install_without_using(&models, "whisper-small"); // installed, but Talaan's settings don't point to it
        let before = |id| matches!(models.status(id), Status::Installed { used_by_me: false, .. });
        assert!(before("parakeet-ultra"));
        let settings = json!({"activeSpeechModel": "parakeet-ultra", "separateSpeakers": false, "summaryEngine": "byok"});
        models.claim(Some(&settings));
        assert!(matches!(models.status("parakeet-ultra"), Status::Installed { used_by_me: true, .. }));
        assert!(before("whisper-small"));
        // Not installed: nothing to record, and no panic.
        assert!(matches!(models.status("vad"), Status::Missing));
    }

    #[test]
    fn start_claims_what_another_app_installed() {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        install_without_using(&models, "whisper-turbo");
        models.start(Some(&json!({"separateSpeakers": false, "summaryEngine": "byok"})));
        assert!(matches!(models.status("whisper-turbo"), Status::Installed { used_by_me: true, .. }));
    }

    #[test]
    fn without_a_settings_file_nothing_is_claimed() {
        // A wipe deletes the settings: the defaults must not make Talaan a user of what is still installed.
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        for id in ["whisper-turbo", "vad", "diarization", "qwen3.5-4b"] {
            install_without_using(&models, id);
        }
        models.claim(None);
        for id in ["whisper-turbo", "vad", "diarization", "qwen3.5-4b"] {
            assert!(matches!(models.status(id), Status::Installed { used_by_me: false, .. }), "{id}");
        }
        // Starting up then tidies away the leftovers nobody uses, instead of keeping them for Talaan.
        models.start(None);
        assert!(matches!(models.status("whisper-turbo"), Status::Missing));
        for id in ["whisper-turbo", "vad", "diarization", "qwen3.5-4b"] {
            install_without_using(&models, id);
        }
        // With a settings file the same models are claimed.
        models.claim(Some(&json!({})));
        assert!(matches!(models.status("whisper-turbo"), Status::Installed { used_by_me: true, .. }));
    }

    #[test]
    fn the_old_folders_are_the_engines_list_for_each_platform() {
        let (local, roaming) = (Path::new("/L"), Path::new("/R"));
        assert_eq!(legacy_dirs_in(local, Some(roaming), false), vec![Path::new("/L/Talaan/models")]);
        let id = App::Talaan.bundle_id();
        assert_eq!(legacy_dirs_in(local, Some(roaming), true), vec![Path::new("/L").join(id).join("models"), Path::new("/R/Talaan/models").to_path_buf()]);
        // A test run has none, so it can't reach the real ones.
        let data = tempfile::tempdir().unwrap();
        assert!(Models::open(Some(data.path())).unwrap().legacy_dirs().is_empty());
    }

    #[test]
    fn deleting_all_data_removes_the_old_folders_and_only_those() {
        let data = tempfile::tempdir().unwrap();
        let (local, roaming) = (data.path().join("local"), data.path().join("roaming"));
        let models = Models::open(Some(data.path())).unwrap().with_legacy(legacy_dirs_in(&local, Some(&roaming), true));
        let old_local = local.join(App::Talaan.bundle_id()).join("models/whisper-small/model.bin");
        let old_roaming = roaming.join("Talaan/models/vad/x");
        let neighbour = local.join(App::Talaan.bundle_id()).join("EBWebView/keep");
        for p in [&old_local, &old_roaming, &neighbour] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        models.remove_legacy_dirs();
        assert!(!old_local.exists() && !old_roaming.exists());
        assert!(neighbour.exists(), "only the models folder goes");
        models.remove_legacy_dirs(); // already gone: fine
    }

    #[test]
    fn starting_on_a_test_folder_changes_nothing_and_does_not_panic() {
        let data = tempfile::tempdir().unwrap();
        let models = Models::open(Some(data.path())).unwrap();
        models.start(None);
        models.start(Some(&json!({"activeSpeechModel": "parakeet-ultra"})));
        assert!(models.ready_dir("vad").is_none());
        assert!(models.ready_dir("parakeet-ultra").is_none());
    }

    /// The uninstaller removes Talaan's old folders and nothing of the shared models folder (Ensayo and Parlato
    /// use it). The file never names that folder's publisher directory, so no line can remove it.
    #[test]
    fn the_uninstaller_never_touches_the_shared_models_folder() {
        let nsh = include_str!("../windows/installer-hooks.nsh");
        assert!(!nsh.contains("Craft Concepts Digital"));
        assert!(nsh.contains(r#"RmDir /r "$APPDATA\Talaan""#));
        assert!(nsh.contains(r#"RmDir /r "$LOCALAPPDATA\${BUNDLEID}\models""#));
        // Every folder it removes is Talaan's own.
        for line in nsh.lines().filter(|l| l.trim_start().starts_with("RmDir")) {
            assert!(line.contains(r"$APPDATA\Talaan") || line.contains(r"${BUNDLEID}"), "{line}");
        }
    }
}
