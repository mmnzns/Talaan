//! Summaries: one fixed prompt and schema for every engine, robust parsing, long calls split and merged.

// Wired into the app in task 7; until then some items are only used by tests.
#![allow(dead_code)]

pub mod chunk;
pub mod engines;
pub mod local;
pub mod parse;
pub mod prompt;
pub mod summarize;
