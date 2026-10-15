//! Notion: connect with an integration secret (kept in secure key storage), list where it can write, export.

use super::{AppState, CmdResult};
use crate::i18n::{tr, tr_err};
use crate::keys;
use crate::notion::{blocks, Destination, Notion, API, KEY_ID};
use crate::storage::StorageError;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

fn refused(m: impl Into<String>) -> StorageError {
    StorageError::Refused(m.into())
}

fn client() -> CmdResult<Notion> {
    let secret = keys::get(KEY_ID).ok_or_else(|| refused(tr("notion.connect_first")))?;
    Ok(Notion::new(API, &secret))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| refused(tr_err("notion.stopped", e)))?
}

#[derive(Serialize)]
pub struct Connected {
    workspace: String,
}

/// Checks the secret with Notion, then saves it. Returns the workspace name.
#[tauri::command]
pub async fn notion_connect(secret: String) -> CmdResult<Connected> {
    blocking(move || {
        let secret = secret.trim().to_string();
        if secret.is_empty() {
            return Err(refused(tr("notion.paste_secret")));
        }
        let workspace = Notion::new(API, &secret).whoami().map_err(refused)?;
        keys::set(KEY_ID, &secret).map_err(refused)?;
        Ok(Connected { workspace })
    })
    .await
}

#[tauri::command]
pub fn notion_disconnect() -> CmdResult<()> {
    keys::clear(KEY_ID).map_err(refused)
}

#[tauri::command]
pub async fn notion_destinations() -> CmdResult<Vec<Destination>> {
    blocking(|| client()?.destinations().map_err(refused)).await
}

#[derive(Deserialize)]
pub struct Target {
    id: String,
    kind: String,
    title: String,
}

/// Sends the meeting (as the user sees it, edits included) and returns the new page's URL.
#[tauri::command]
pub async fn notion_export(app: AppHandle, id: String, destination: Target, include_transcript: bool) -> CmdResult<String> {
    let meeting = app.state::<AppState>().lock().get_meeting(&id)?;
    blocking(move || {
        let dest = Destination { id: destination.id, kind: destination.kind, title: destination.title };
        client()?.export(&dest, &meeting.item.title, &blocks(&meeting, include_transcript)).map_err(refused)
    })
    .await
}
