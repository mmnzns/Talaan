//! Meeting storage: plain folders on disk, readable without Talaan. No Tauri types in here.

pub mod edits;
pub mod io;
pub mod library;
pub mod markdown;
pub mod model;
pub mod ops;
pub mod paths;
pub mod recovery;
pub mod retention;
pub mod store;

pub use store::{Filter, Storage};

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not available yet: {0}")]
    NotYetAvailable(String),
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
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) | Self::InvalidId(_) => "not_found",
            Self::NotYetAvailable(_) => "not_yet_available",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::Io(_) | Self::Json(_) => "io",
        }
    }
}

/// Errors cross the bridge as `{ code, message }`.
impl Serialize for StorageError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("StorageError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
