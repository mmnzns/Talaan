//! Check setup › Graphics chip: whether Talaan uses the GPU, on what, and if not, why (with Try again).

use super::transcriber::TranscriberState;
use super::AppState;
use crate::gpu::{Gpu, Kind, Plan, Skip};
use serde::Serialize;
use voice_engine_client::LlmClient;
use voice_engine_protocol::Backend;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    /// "metal" (the Mac) | "in_use" | "off" | "unavailable" | "failed"
    pub state: &'static str,
    /// When unavailable: "no_vulkan" | "no_helper" | "no_gpu" | "no_start".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
    /// The technical error behind "failed" or "no_start" (first line, English).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What a test start of the GPU helper found.
pub enum Probe {
    Ready(Backend),
    Failed(String),
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    let mut cut = line.len().min(160);
    while !line.is_char_boundary(cut) {
        cut -= 1;
    }
    line[..cut].to_string()
}

/// The status from the plans for both jobs and, when they say GPU, a test start of the summary GPU helper (both
/// helpers choose the device the same way).
pub fn status(mac: bool, use_gpu: bool, llm: Plan, stt: Plan, probe: impl FnOnce(&Plan) -> Probe) -> GpuStatus {
    let unavailable = |reason| GpuStatus { state: "unavailable", reason: Some(reason), ..Default::default() };
    if mac {
        return GpuStatus { state: if use_gpu { "metal" } else { "off" }, ..Default::default() };
    }
    if !use_gpu {
        return GpuStatus { state: "off", ..Default::default() };
    }
    for plan in [&llm, &stt] {
        match plan {
            Plan::Cpu(Skip::NoVulkan) => return unavailable("no_vulkan"),
            Plan::Cpu(Skip::NoHelper) => return unavailable("no_helper"),
            _ => {}
        }
    }
    if let Some(reason) = [&llm, &stt].into_iter().find_map(|p| match p {
        Plan::Cpu(Skip::Failed(r)) => Some(r.clone()),
        _ => None,
    }) {
        return GpuStatus { state: "failed", detail: Some(first_line(&reason)), ..Default::default() };
    }
    match probe(&llm) {
        Probe::Ready(b) if b.on_gpu() => GpuStatus { state: "in_use", device: b.device, vram_mb: Some(b.vram_mb), ..Default::default() },
        Probe::Ready(_) => unavailable("no_gpu"),
        Probe::Failed(e) => GpuStatus { detail: Some(first_line(&e)), ..unavailable("no_start") },
    }
}

/// Asks the engine's client which graphics card the summary helper would use. Until Task 7 rewrites this screen the
/// probe is only that question (`discrete_card`); it starts the helper if none runs, which the next summary reuses.
fn probe_helper(client: &LlmClient, plan: &Plan) -> Probe {
    if !matches!(plan, Plan::Gpu { .. }) {
        return Probe::Failed("no GPU helper".into());
    }
    match client.discrete_card() {
        Ok(Some(d)) => Probe::Ready(Backend { backend: "vulkan".into(), device: Some(d.description), vram_mb: d.memory_mb }),
        Ok(None) => Probe::Ready(Backend::cpu()),
        Err(e) => Probe::Failed(format!("{e:?}")),
    }
}

fn current(app: &AppHandle) -> GpuStatus {
    let use_gpu = app.state::<AppState>().lock().settings_get().ok().flatten().and_then(|s| s["useGpu"].as_bool()).unwrap_or(true);
    let data_dir = app.state::<TranscriberState>().data_dir().to_path_buf();
    let gpu = Gpu::shared(&data_dir);
    let client = crate::summarizer::local::summary_client(&data_dir);
    status(cfg!(target_os = "macos"), use_gpu, gpu.plan(Kind::Llm, use_gpu), gpu.plan(Kind::Stt, use_gpu), |plan| probe_helper(&client, plan))
}

/// The Graphics chip row. Starts the GPU helper for a moment, so it runs off the main thread.
#[tauri::command]
pub async fn gpu_status(app: AppHandle) -> GpuStatus {
    tauri::async_runtime::spawn_blocking(move || current(&app)).await.unwrap_or(GpuStatus { state: "unavailable", reason: Some("no_start"), ..Default::default() })
}

/// Try again: forget remembered GPU failures, then check again.
#[tauri::command]
pub async fn gpu_retry(app: AppHandle) -> GpuStatus {
    tauri::async_runtime::spawn_blocking(move || {
        Gpu::shared(app.state::<TranscriberState>().data_dir()).clear();
        current(&app)
    })
    .await
    .unwrap_or(GpuStatus { state: "unavailable", reason: Some("no_start"), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gpu() -> Plan {
        Plan::Gpu { exe: PathBuf::from("talaan-llm-vulkan.exe"), env: vec![] }
    }
    fn rtx() -> Probe {
        Probe::Ready(Backend { backend: "vulkan".into(), device: Some("NVIDIA GeForce RTX 5070 Ti".into()), vram_mb: 16303 })
    }
    fn never(_: &Plan) -> Probe {
        panic!("no probe expected")
    }

    #[test]
    fn the_mac_reports_metal_without_probing() {
        assert_eq!(status(true, true, Plan::Cpu(Skip::NoVulkan), Plan::Cpu(Skip::NoVulkan), never).state, "metal");
        assert_eq!(status(true, false, gpu(), gpu(), never).state, "off");
    }

    #[test]
    fn windows_names_the_gpu_in_use() {
        let s = status(false, true, gpu(), gpu(), |_| rtx());
        assert_eq!((s.state, s.device.as_deref(), s.vram_mb), ("in_use", Some("NVIDIA GeForce RTX 5070 Ti"), Some(16303)));
        assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({ "state": "in_use", "device": "NVIDIA GeForce RTX 5070 Ti", "vramMb": 16303 }));
    }

    #[test]
    fn windows_says_why_not() {
        assert_eq!(status(false, false, gpu(), gpu(), never).state, "off");
        assert_eq!(status(false, true, Plan::Cpu(Skip::NoVulkan), Plan::Cpu(Skip::NoVulkan), never).reason, Some("no_vulkan"));
        assert_eq!(status(false, true, Plan::Cpu(Skip::NoHelper), gpu(), never).reason, Some("no_helper"));
        let failed = status(false, true, gpu(), Plan::Cpu(Skip::Failed("speech GPU helper stopped: exit code 5\nggml_vulkan: Device lost".into())), never);
        assert_eq!((failed.state, failed.detail.as_deref()), ("failed", Some("speech GPU helper stopped: exit code 5")));
        let igpu_only = status(false, true, gpu(), gpu(), |_| Probe::Ready(Backend::cpu()));
        assert_eq!((igpu_only.state, igpu_only.reason), ("unavailable", Some("no_gpu")));
        let broken = status(false, true, gpu(), gpu(), |_| Probe::Failed("GPU helper stopped: exit code -1073741515 (0xC0000135)".into()));
        assert_eq!((broken.state, broken.reason, broken.detail.as_deref()), ("unavailable", Some("no_start"), Some("GPU helper stopped: exit code -1073741515 (0xC0000135)")));
    }

    #[test]
    fn the_probe_says_so_when_the_helper_will_not_start() {
        let dir = tempfile::tempdir().unwrap();
        let client = crate::summarizer::local::client(dir.path(), "test", "summary");
        let Probe::Failed(e) = probe_helper(&client, &Plan::Cpu(Skip::Off)) else { panic!() };
        assert_eq!(e, "no GPU helper");
        // A helper that is missing (the real one is next to the test binary only after build-llm-helper.sh) gives
        // either an answer or a failure, never a hang or a panic.
        let _ = probe_helper(&client, &gpu());
        client.shutdown();
    }
}
