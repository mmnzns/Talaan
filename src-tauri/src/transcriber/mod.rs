//! Speech to text: models, voice activity detection, engines, merging the two tracks, echo removal.

// Parts are wired into the app in task 7; until then some items are only used by tests.
#![allow(dead_code)]

pub mod download;
pub mod models;
