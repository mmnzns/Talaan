use super::{notify, AppState, CmdResult, MEETINGS_CHANGED};
use crate::storage::model::{Meeting, MeetingListItem, Summary};
use crate::storage::library::Folder;
use crate::storage::{Filter, StorageError};
use serde::Deserialize;
use tauri::{AppHandle, State};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilterArg {
    folder_id: Option<String>,
    tag: Option<String>,
    query: Option<String>,
}

#[tauri::command]
pub fn meetings_list(state: State<AppState>, filter: Option<FilterArg>) -> CmdResult<Vec<MeetingListItem>> {
    let f = filter.unwrap_or_default();
    state.lock().list_meetings(&Filter { folder_id: f.folder_id, tag: f.tag, query: f.query })
}

/// `None` (null in JS) when the meeting doesn't exist, so the UI can show "not found".
#[tauri::command]
pub fn meetings_get(state: State<AppState>, id: String) -> CmdResult<Option<Meeting>> {
    match state.lock().get_meeting(&id) {
        Ok(m) => Ok(Some(m)),
        Err(StorageError::NotFound(_)) | Err(StorageError::InvalidId(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn changed<T>(app: &AppHandle, r: CmdResult<T>) -> CmdResult<T> {
    if r.is_ok() {
        notify(app, MEETINGS_CHANGED);
    }
    r
}

#[tauri::command]
pub fn meetings_rename(app: AppHandle, state: State<AppState>, id: String, title: String) -> CmdResult<()> {
    changed(&app, state.lock().rename_meeting(&id, &title))
}

#[tauri::command]
pub fn meetings_set_pinned(app: AppHandle, state: State<AppState>, id: String, pinned: bool) -> CmdResult<()> {
    changed(&app, state.lock().set_pinned(&id, pinned))
}

#[tauri::command]
pub fn meetings_delete(app: AppHandle, state: State<AppState>, t: State<super::transcriber::TranscriberState>, id: String) -> CmdResult<()> {
    t.worker.cancel_if_running(&id);
    changed(&app, state.lock().delete_meeting(&id))
}

#[tauri::command]
pub fn meetings_update_transcript_line(app: AppHandle, state: State<AppState>, id: String, line_id: String, text: String) -> CmdResult<()> {
    changed(&app, state.lock().update_transcript_line(&id, &line_id, &text))
}

#[tauri::command]
pub fn meetings_update_summary(app: AppHandle, state: State<AppState>, id: String, summary: Summary) -> CmdResult<()> {
    changed(&app, state.lock().update_summary(&id, &summary))
}

#[tauri::command]
pub fn meetings_revert_edits(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    changed(&app, state.lock().revert_edits(&id))
}

#[tauri::command]
pub fn meetings_rename_speaker(app: AppHandle, state: State<AppState>, id: String, speaker: String, name: String) -> CmdResult<()> {
    changed(&app, state.lock().rename_speaker(&id, &speaker, &name))
}

#[tauri::command]
pub fn meetings_set_tags(app: AppHandle, state: State<AppState>, id: String, tags: Vec<String>) -> CmdResult<()> {
    changed(&app, state.lock().set_tags(&id, &tags))
}

#[tauri::command]
pub fn meetings_move_to_folder(app: AppHandle, state: State<AppState>, id: String, folder_id: Option<String>) -> CmdResult<()> {
    changed(&app, state.lock().move_to_folder(&id, folder_id.as_deref()))
}

#[tauri::command]
pub fn folders_list(state: State<AppState>) -> CmdResult<Vec<Folder>> {
    state.lock().list_folders()
}

#[tauri::command]
pub fn folders_create(app: AppHandle, state: State<AppState>, name: String) -> CmdResult<Folder> {
    changed(&app, state.lock().create_folder(&name))
}

#[tauri::command]
pub fn tags_list(state: State<AppState>) -> CmdResult<Vec<String>> {
    state.lock().list_tags()
}

// Needs the summarizer (M5). Until then the UI shows a short notice.
#[tauri::command]
pub fn meetings_generate_summary(_id: String) -> CmdResult<()> {
    Err(StorageError::NotYetAvailable("Writing summaries needs the summarizer (M5)".into()))
}
