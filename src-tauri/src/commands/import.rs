//! Import audio: decode in the background into a temporary file, and only then create the meeting (so a bad
//! file, a cancel or a crash never leaves a half-made meeting). The meeting then goes through transcribe + summarize.

use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::importer::{guard_panic, import_to_wav, temp_name, CutOff, TempFile};
use crate::storage::model::MeetingStatus;
use crate::storage::paths::THEM_WAV;
use crate::storage::StorageError;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "camelCase")]
enum ImportEvent {
    Progress { progress: f32 },
    #[serde(rename_all = "camelCase")]
    Done { id: String, cut_off: Option<CutOff> },
    Failed { message: String },
    Cancelled,
}

const EVENT: &str = "import-audio";

/// The import in progress (one at a time; the screen queues files), so Cancel can reach it.
#[derive(Default)]
pub struct ImportState {
    current: Mutex<Option<Arc<AtomicBool>>>,
    seq: AtomicU64,
}

impl ImportState {
    fn begin(&self) -> (Arc<AtomicBool>, u64) {
        let flag = Arc::new(AtomicBool::new(false));
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(flag.clone());
        (flag, self.seq.fetch_add(1, Ordering::Relaxed))
    }
    fn end(&self, flag: &Arc<AtomicBool>) {
        let mut cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        if cur.as_ref().is_some_and(|c| Arc::ptr_eq(c, flag)) {
            *cur = None;
        }
    }
    fn cancel(&self) {
        if let Some(f) = self.current.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            f.store(true, Ordering::Relaxed);
        }
    }
}

/// Starts importing `path`; progress and the result arrive as `import-audio` events.
#[tauri::command]
pub fn meetings_import(app: AppHandle, path: String) -> CmdResult<()> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err(StorageError::Refused(crate::i18n::tr("import.file_gone")));
    }
    let (cancel, seq) = app.state::<ImportState>().begin();
    std::thread::spawn(move || {
        let send = |e: ImportEvent| {
            match &e {
                ImportEvent::Done { id, .. } => log::info!("audio imported as meeting {id}"),
                ImportEvent::Failed { message } => log::warn!("import failed: {}", crate::logging::text(message)),
                ImportEvent::Cancelled => log::info!("import cancelled"),
                ImportEvent::Progress { .. } => {}
            }
            let _ = app.emit(EVENT, e);
        };
        // A panic anywhere in here must still end with an event, or the screen waits on "Reading…" for good.
        let outcome = guard_panic(|| run(&app, &src, &cancel, seq, &send));
        app.state::<ImportState>().end(&cancel);
        match outcome {
            Ok(event) => send(event),
            Err(message) if message == "cancelled" => send(ImportEvent::Cancelled),
            Err(message) => send(ImportEvent::Failed { message }),
        }
    });
    Ok(())
}

/// Stops the import in progress. The file being read leaves nothing behind; the screen drops what's still queued.
#[tauri::command]
pub fn meetings_import_cancel(app: AppHandle) {
    app.state::<ImportState>().cancel();
}

fn run(app: &AppHandle, src: &std::path::Path, cancel: &AtomicBool, seq: u64, send: &dyn Fn(ImportEvent)) -> Result<ImportEvent, String> {
    let root = app.state::<AppState>().lock().root().to_path_buf();
    let tmp = temp_name(&root, seq);
    // Removed on every way out except the move into the meeting.
    let mut tmp_guard = TempFile::new(&tmp);
    let mut last = Instant::now();
    let imported = import_to_wav(src, &tmp, cancel, &mut |p| {
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            send(ImportEvent::Progress { progress: p });
        }
    })?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let duration = imported.duration_ms;
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "audio".into());
    // The file's date is when it was recorded (it ends there), which is what the list should show.
    let ended = std::fs::metadata(src).and_then(|m| m.modified()).map(chrono::DateTime::<chrono::Local>::from).unwrap_or_else(|_| chrono::Local::now());
    let started = (ended - chrono::Duration::milliseconds(duration as i64)).fixed_offset();
    let state = app.state::<AppState>();
    let storage = state.lock();
    let made = storage.create_meeting("import", "Imported", started).and_then(|f| {
        let dir = storage.dir(&f.id)?;
        std::fs::rename(&tmp, dir.join(THEM_WAV))?;
        tmp_guard.keep();
        storage.update_meeting_file(&f.id, |m| {
            m.title = format!("Imported: {name}");
            // Retention counts from now, whatever the file's date says.
            m.imported_at = Some(chrono::Local::now().to_rfc3339());
            m.duration_ms = duration;
            m.status = MeetingStatus::Transcribing;
            m.progress = Some(0.0);
        })?;
        Ok(f.id)
    });
    drop(storage);
    let id = made.map_err(|e| crate::i18n::tr_err("import.save_failed", e))?;
    app.state::<super::transcriber::TranscriberState>().enqueue(&id);
    notify(app, MEETINGS_CHANGED);
    Ok(ImportEvent::Done { id, cut_off: imported.cut_off })
}

/// Extensions for the file picker.
#[tauri::command]
pub fn import_extensions() -> Vec<&'static str> {
    crate::importer::EXTENSIONS.to_vec()
}
