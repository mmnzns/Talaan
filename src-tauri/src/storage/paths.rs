//! On-disk layout, meeting ids and placeholder titles.

use chrono::{DateTime, FixedOffset};
use std::path::{Path, PathBuf};

pub const MEETINGS_DIR: &str = "meetings";
pub const META: &str = "meeting.json";
pub const ME_WAV: &str = "me.wav";
pub const THEM_WAV: &str = "them.wav";
pub const TRANSCRIPT: &str = "transcript.json";
pub const TRANSCRIPT_EDITS: &str = "transcript.edits.json";
pub const TRANSCRIPT_MD: &str = "transcript.md";
pub const SUMMARY: &str = "summary.json";
pub const SUMMARY_EDITED: &str = "summary.edited.json";
pub const SUMMARY_MD: &str = "summary.md";
pub const SUMMARY_DONE: &str = "summary.done.json";
pub const LIBRARY: &str = "library.json";
pub const SETTINGS: &str = "settings.json";

pub fn meetings_dir(root: &Path) -> PathBuf {
    root.join(MEETINGS_DIR)
}

pub fn meeting_dir(root: &Path, id: &str) -> PathBuf {
    meetings_dir(root).join(id)
}

/// Ids come from the frontend, so they must never be able to leave the meetings folder.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `2026-10-14_1430_k3f9`: local start time, then four random characters. Sorts by time and reads well in Finder.
pub fn new_meeting_id(started: &DateTime<FixedOffset>) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut n = rand::random::<u32>();
    let suffix: String = (0..4)
        .map(|_| {
            let c = ALPHABET[(n % 36) as usize] as char;
            n /= 36;
            c
        })
        .collect();
    format!("{}_{suffix}", started.format("%Y-%m-%d_%H%M"))
}

/// `Call, Zoom, Oct 14, 2:30 PM`. Replaced by a title from the summary once it exists.
pub fn placeholder_title(app_label: &str, started: &DateTime<FixedOffset>) -> String {
    format!("Call, {app_label}, {}", started.format("%b %-d, %-I:%M %p"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()
    }

    #[test]
    fn ids_sort_by_time_and_have_a_random_suffix() {
        let id = new_meeting_id(&at());
        assert!(id.starts_with("2026-10-14_1430_"), "{id}");
        let suffix = id.rsplit('_').next().unwrap();
        assert_eq!(suffix.len(), 4);
        assert!(suffix.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert!(is_valid_id(&id));
    }

    #[test]
    fn ids_differ_between_calls() {
        let ids: std::collections::HashSet<_> = (0..50).map(|_| new_meeting_id(&at())).collect();
        assert!(ids.len() > 40);
    }

    #[test]
    fn placeholder_title_reads_naturally() {
        assert_eq!(placeholder_title("Zoom", &at()), "Call, Zoom, Oct 14, 2:30 PM");
        let morning = DateTime::parse_from_rfc3339("2026-10-04T09:05:00-07:00").unwrap();
        assert_eq!(placeholder_title("Slack huddle", &morning), "Call, Slack huddle, Oct 4, 9:05 AM");
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for bad in ["", "../x", "a/b", "a\\b", "..", ".", "a b", &"x".repeat(65)] {
            assert!(!is_valid_id(bad), "{bad:?}");
        }
        assert!(is_valid_id("2026-10-14_1430_k3f9"));
    }
}
