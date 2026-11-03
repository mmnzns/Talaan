//! Check setup › Graphics chip: whether Talaan uses the graphics card, on what, and if it failed (with Try again).
//! No helper starts for this: the card comes from a cheap listing and failures from the engine's memory files.

use super::transcriber::TranscriberState;
use super::AppState;
use crate::gpu::FellBack;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager};
use voice_engine_client::clear_gpu_failure;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    /// "metal" (the Mac) | "in_use" | "off" | "unavailable" | "failed"
    pub state: &'static str,
    /// When unavailable: "no_gpu" (no separate graphics card).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
    /// For `failed`, which jobs fell back to the processor, as codes the screen words: "stt", "llm" or "stt,llm".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Which jobs the card failed for, as codes: speech ("stt"), AI for summaries and search ("llm"), or both.
fn failed_parts(stt: bool, llm: bool) -> Option<String> {
    let parts: Vec<&str> = [(stt, "stt"), (llm, "llm")].into_iter().filter(|p| p.0).map(|p| p.1).collect();
    (!parts.is_empty()).then(|| parts.join(","))
}

pub fn status(mac: bool, use_gpu: bool, card: Option<&str>, stt_failed: bool, llm_failed: bool) -> GpuStatus {
    if !use_gpu {
        return GpuStatus { state: "off", ..Default::default() };
    }
    if let Some(detail) = failed_parts(stt_failed, llm_failed) {
        return GpuStatus { state: "failed", detail: Some(detail), ..Default::default() };
    }
    if mac {
        return GpuStatus { state: "metal", ..Default::default() };
    }
    match card {
        Some(name) => GpuStatus { state: "in_use", device: Some(name.to_string()), ..Default::default() },
        None => GpuStatus { state: "unavailable", reason: Some("no_gpu"), ..Default::default() },
    }
}

/// Whether the engine remembers a card failure for this app version, for speech (Windows Whisper) and for local AI.
pub fn remembered(data_dir: &Path) -> (bool, bool) {
    let stt = crate::transcriber::helper_speech::stt_card_failed(data_dir, env!("CARGO_PKG_VERSION"));
    let llm = ["summary", "search"].iter().any(|w| crate::summarizer::local::card_failed(data_dir, w));
    (stt, llm)
}

/// Whether local AI should be counted as running on the card (for time estimates).
pub fn llm_on_gpu(mac: bool, use_gpu: bool, data_dir: &Path) -> bool {
    use_gpu && (mac || crate::gpu::discrete_card().is_some()) && !remembered(data_dir).1 && crate::gpu::fallbacks().llm.is_none()
}

fn current(app: &AppHandle) -> GpuStatus {
    let use_gpu = app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s["useGpu"].as_bool()).unwrap_or(true);
    let data_dir = app.state::<TranscriberState>().data_dir().to_path_buf();
    let (stt_file, llm_file) = remembered(&data_dir);
    let FellBack { stt, llm } = crate::gpu::fallbacks();
    let card = crate::gpu::discrete_card();
    status(cfg!(target_os = "macos"), use_gpu, card.as_deref(), stt_file || stt.is_some(), llm_file || llm.is_some())
}

/// The Graphics chip row.
#[tauri::command(async)]
pub fn gpu_status(app: AppHandle) -> GpuStatus {
    current(&app)
}

/// Try again: forget remembered card failures (the engine's files and this run's note), then check again.
#[tauri::command(async)]
pub fn gpu_retry(app: AppHandle) -> GpuStatus {
    let data_dir = app.state::<TranscriberState>().data_dir().to_path_buf();
    for file in [crate::transcriber::helper_speech::FAILURES_FILE.to_string(), crate::summarizer::local::failures_file("summary"), crate::summarizer::local::failures_file("search")] {
        clear_gpu_failure(&data_dir.join(file));
    }
    crate::gpu::clear_fallbacks();
    // The files above are all a client built later needs. A running client also remembers the failure in memory:
    // `retry_gpu` clears that and stops the processor helper that took over, so its next request starts on the card.
    // It waits for a request in progress (a summary can take minutes), so it runs aside: the status below doesn't
    // depend on it.
    let retry_app = app.clone();
    std::thread::spawn(move || {
        crate::summarizer::local::retry_gpu_all();
        #[cfg(windows)]
        retry_app.state::<TranscriberState>().retry_speech_gpu();
        #[cfg(not(windows))]
        let _ = retry_app;
    });
    current(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RTX: Option<&str> = Some("NVIDIA GeForce RTX 5070 Ti");

    #[test]
    fn each_state_shows_when_it_should() {
        assert_eq!(status(true, true, None, false, false).state, "metal");
        assert_eq!(status(true, false, None, false, false).state, "off");
        let s = status(false, true, RTX, false, false);
        assert_eq!((s.state, s.device.as_deref()), ("in_use", RTX));
        assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({ "state": "in_use", "device": "NVIDIA GeForce RTX 5070 Ti" }));
        let none = status(false, true, None, false, false);
        assert_eq!((none.state, none.reason), ("unavailable", Some("no_gpu")));
        assert_eq!(status(false, false, RTX, false, false).state, "off");
    }

    #[test]
    fn a_failed_card_says_which_part_used_the_processor() {
        let both = status(false, true, RTX, true, true);
        assert_eq!((both.state, both.detail.as_deref()), ("failed", Some("stt,llm")));
        assert_eq!(status(false, true, RTX, true, false).detail.as_deref(), Some("stt"));
        assert_eq!(status(true, true, None, false, true).detail.as_deref(), Some("llm"));
        // Turned off, nothing is "failed".
        assert_eq!(status(false, false, RTX, true, true).state, "off");
    }

    #[test]
    fn remembered_failures_are_read_for_this_version_and_cleared_files_do_not_count() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(remembered(dir.path()), (false, false));
        std::fs::write(dir.path().join(crate::summarizer::local::failures_file("search")), env!("CARGO_PKG_VERSION")).unwrap();
        assert_eq!(remembered(dir.path()), (false, true));
        std::fs::write(dir.path().join(crate::transcriber::helper_speech::FAILURES_FILE), "0.0.0-other").unwrap();
        assert_eq!(remembered(dir.path()), (false, true));
    }
}
