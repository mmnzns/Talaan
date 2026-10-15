//! Import audio: decode in the background into a temporary file, and only then create the meeting (so a bad
//! file or a crash never leaves a half-made meeting). The meeting then goes through transcribe + summarize.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::importer::import_to_wav;
use crate::storage::model::MeetingStatus;
use crate::storage::paths::THEM_WAV;
use crate::storage::StorageError;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "camelCase")]
enum ImportEvent {
    Progress { progress: f32 },
    Done { id: String },
    Failed { message: String },
}

const EVENT: &str = "import-audio";

/// Starts importing `path`; progress and the result arrive as `import-audio` events.
#[tauri::command]
pub fn meetings_import(app: AppHandle, path: String) -> CmdResult<()> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err(StorageError::Refused(crate::i18n::tr("import.file_gone")));
    }
    std::thread::spawn(move || {
        let send = |e: ImportEvent| {
            let _ = app.emit(EVENT, e);
        };
        let root = app.state::<AppState>().lock().root().to_path_buf();
        let tmp = root.join(format!("import-{}.wav", std::process::id()));
        let mut last = Instant::now();
        let result = import_to_wav(&src, &tmp, &AtomicBool::new(false), &mut |p| {
            if last.elapsed() > Duration::from_millis(200) {
                last = Instant::now();
                send(ImportEvent::Progress { progress: p });
            }
        });
        let duration = match result {
            Ok(ms) => ms,
            Err(message) => {
                let _ = std::fs::remove_file(&tmp);
                return send(ImportEvent::Failed { message });
            }
        };
        let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "audio".into());
        // The file's date is when it was recorded (it ends there), which is what the list should show.
        let ended = std::fs::metadata(&src).and_then(|m| m.modified()).map(chrono::DateTime::<chrono::Local>::from).unwrap_or_else(|_| chrono::Local::now());
        let started = (ended - chrono::Duration::milliseconds(duration as i64)).fixed_offset();
        let state = app.state::<AppState>();
        let storage = state.lock();
        let made = storage.create_meeting("import", "Imported", started).and_then(|f| {
            let dir = storage.dir(&f.id)?;
            std::fs::rename(&tmp, dir.join(THEM_WAV))?;
            storage.update_meeting_file(&f.id, |m| {
                m.title = format!("Imported: {name}");
                m.duration_ms = duration;
                m.status = MeetingStatus::Transcribing;
                m.progress = Some(0.0);
            })?;
            Ok(f.id)
        });
        drop(storage);
        match made {
            Ok(id) => {
                app.state::<super::transcriber::TranscriberState>().enqueue(&id);
                notify(&app, MEETINGS_CHANGED);
                send(ImportEvent::Done { id });
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                send(ImportEvent::Failed { message: crate::i18n::tr_err("import.save_failed", e) });
            }
        }
    });
    Ok(())
}

/// Extensions for the file picker.
#[tauri::command]
pub fn import_extensions() -> Vec<&'static str> {
    crate::importer::EXTENSIONS.to_vec()
}
