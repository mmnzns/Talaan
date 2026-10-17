//! Meeting storage: plain folders on disk, readable without Talaan. No Tauri types in here.
//!
//! Some of the API (create, save_transcript, save_summary, set_status) is only called from M3 onward.
#![allow(dead_code)]

pub mod dev;
pub mod edits;
pub mod io;
pub mod library;
pub mod markdown;
pub mod model;
pub mod ops;
pub mod paths;
pub mod recovery;
pub mod retention;
pub mod settings;
pub mod speed;
pub mod store;

pub use store::{Filter, Storage};

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    NotYetAvailable(String),
    /// The action can't be done right now; the message says why, in words for the user.
    #[error("{0}")]
    Refused(String),
    #[error("meeting {id} was written by a newer Talaan (schema {found})")]
    UnsupportedSchema { id: String, found: u32 },
    #[error("invalid id: {0}")]
    InvalidId(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl StorageError {
    /// What the log may say about this error. A refusal is a message for the user that can quote a provider, Notion
    /// or a model, so only its i18n key is kept (or nothing, for text that isn't one of the app's messages); local
    /// errors (files, JSON, ids) are redacted.
    pub fn for_log(&self) -> String {
        match self {
            Self::Refused(m) | Self::NotYetAvailable(m) => crate::i18n::key_for(m).map(|k| format!("[{k}]")).unwrap_or_else(|| "(message not logged)".into()),
            other => crate::logging::redact(&other.to_string()),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) | Self::InvalidId(_) => "not_found",
            Self::NotYetAvailable(_) => "not_yet_available",
            Self::Refused(_) => "refused",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::Io(_) | Self::Json(_) => "io",
        }
    }
}

/// Errors cross the bridge as `{ code, message }`. That is the one way a command's error reaches the screens, so
/// it is logged here: every error the app shows passes through.
impl Serialize for StorageError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        log::warn!("error for the app ({}): {}", self.code(), self.for_log());
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("StorageError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
