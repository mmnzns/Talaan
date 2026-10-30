//! The local AI: the engine's llama.cpp helper through `LlmClient` (voice-engine-client). The client starts the
//! helper when needed, keeps it while the app runs, restarts it when it dies, tries the Windows graphics card helper
//! first and remembers when it failed. Talaan keeps two clients: one for summaries and follow-ups, one for search
//! (Ask), so a long summary never holds up a question.

use super::summarize::EngineError as SummaryError;
use crate::i18n::tr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use voice_engine_client::{EngineError, Limits, LlmClient, LlmConfig};

const CPU_HELPER: &str = "talaan-llm";
/// The Windows Vulkan build, next to the processor one.
const GPU_HELPER: &str = "talaan-llm-vulkan";

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Where the helpers live: next to Talaan's own binary (in the app bundle, `Contents/MacOS/`; next to `Talaan.exe`
/// on Windows; in `target/<profile>/` during development, where a test binary one level down in `deps/` looks one
/// folder up).
pub fn helper_dir() -> PathBuf {
    let mut dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    if dir.file_name().is_some_and(|n| n == "deps") {
        dir.pop();
    }
    dir
}

/// A client for one job (`which`: "summary" or "search"; it names the file that remembers a graphics card failure).
pub fn client(data_dir: &Path, version: &str, which: &str) -> LlmClient {
    let dir = helper_dir();
    // Windows only: the Vulkan helper, tried first until it fails once. Elsewhere the one helper is the Metal build.
    let gpu_helper = cfg!(windows).then(|| dir.join(exe(GPU_HELPER))).filter(|p| p.exists());
    LlmClient::new(LlmConfig {
        cpu_helper: dir.join(exe(CPU_HELPER)),
        gpu_helper,
        env: vec![],
        gpu_env: vec![("VK_LOADER_LAYERS_DISABLE".into(), "~implicit~".into())],
        gpu_failures: Some(data_dir.join(failures_file(which))),
        gpu_key: version.to_string(),
        idle_secs: Some(120),
        // Loading a 9B model from a slow disk takes well under this.
        limits: Limits { stall: Duration::from_secs(5 * 60), ..Limits::default() },
    })
}

/// The two clients, built on first use. Behind a lock so `reset_clients` can drop them: a client remembers in
/// memory that the graphics card failed (`gpu_disabled`), and only a new one tries the card again.
static SUMMARY: Mutex<Option<Arc<LlmClient>>> = Mutex::new(None);
static SEARCH: Mutex<Option<Arc<LlmClient>>> = Mutex::new(None);

fn shared(slot: &Mutex<Option<Arc<LlmClient>>>, data_dir: &Path, which: &str) -> Arc<LlmClient> {
    // The data folder never changes while Talaan runs, so the first caller's folder is the folder.
    slot.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(|| Arc::new(client(data_dir, env!("CARGO_PKG_VERSION"), which))).clone()
}

/// The app's client for summaries and follow-ups.
pub fn summary_client(data_dir: &Path) -> Arc<LlmClient> {
    shared(&SUMMARY, data_dir, "summary")
}

/// The app's client for search (Ask): the embedding model.
pub fn search_client(data_dir: &Path) -> Arc<LlmClient> {
    shared(&SEARCH, data_dir, "search")
}

/// Stops both helpers now (the app is quitting, or an update is about to replace them: Windows can't replace a
/// running exe). Starts nothing that isn't running; `shutdown` kills, so it doesn't hold the caller long.
pub fn shutdown_all() {
    for slot in [&SUMMARY, &SEARCH] {
        if let Some(c) = slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            c.shutdown();
        }
    }
}

/// Stops both helpers and forgets both clients: the next call builds fresh ones, which try the graphics card again
/// ("Try again" in Check setup) and have no model open (Delete). A call already running with an old client ends
/// with an error, as with `shutdown_all`.
pub fn reset_clients() {
    for slot in [&SUMMARY, &SEARCH] {
        let old = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(c) = old {
            c.shutdown();
        }
    }
}

/// Words for the user, from what the engine said. The error itself (`{:?}`, redacted) goes to the log; the caller
/// keeps it for `failure_detail`.
pub fn to_summary_error(e: EngineError) -> SummaryError {
    match e {
        EngineError::Cancelled => SummaryError::Cancelled,
        EngineError::OutOfMemory => SummaryError::Failed(tr("local_ai.out_of_memory")),
        EngineError::Crashed => SummaryError::Failed(tr("local_ai.crashed")),
        EngineError::TimedOut => SummaryError::Failed(tr("local_ai.too_slow")),
        EngineError::CouldNotStart { .. } => SummaryError::Failed(tr("local_ai.no_start")),
        EngineError::ModelMissing { .. } => SummaryError::Unavailable(tr("ai.download_local_short")),
        // PromptTooLong, BadSchema, NoChatTemplate, BadModel, Internal, VersionMismatch, BadRequest, Unsupported,
        // GpuFailed ...: nothing the user can act on.
        other => {
            let detail = format!("{other:?}");
            log::warn!("local AI: {}", crate::logging::text(detail));
            SummaryError::Failed(tr("local_ai.crashed"))
        }
    }
}

/// Notes where a local AI call ran; when the graphics card failed in it (the processor did the job), Settings shows it.
pub fn record_outcome(backend: &str, fell_back: bool) {
    if fell_back {
        crate::gpu::note_llm_fallback(backend);
    }
    log::info!("local AI ran on {backend}{}", if fell_back { " (the graphics card failed; the processor did the job)" } else { "" });
}

/// Whether the engine remembers a card failure for this app version for the job (`which`: "summary" or "search").
pub fn card_failed(data_dir: &Path, which: &str) -> bool {
    std::fs::read_to_string(data_dir.join(failures_file(which))).is_ok_and(|t| t.trim() == env!("CARGO_PKG_VERSION"))
}

pub fn failures_file(which: &str) -> String {
    format!("gpu-{which}-failures")
}

#[cfg(test)]
mod tests {
    use super::*;
    use EngineError as E;

    #[test]
    fn engine_errors_become_talaan_messages() {
        assert_eq!(to_summary_error(E::Cancelled), SummaryError::Cancelled);
        assert_eq!(to_summary_error(E::OutOfMemory), SummaryError::Failed(tr("local_ai.out_of_memory")));
        assert_eq!(to_summary_error(E::Crashed), SummaryError::Failed(tr("local_ai.crashed")));
        assert_eq!(to_summary_error(E::TimedOut), SummaryError::Failed(tr("local_ai.too_slow")));
        assert_eq!(to_summary_error(E::CouldNotStart { message: "x".into() }), SummaryError::Failed(tr("local_ai.no_start")));
        assert_eq!(to_summary_error(E::ModelMissing { path: "x".into() }), SummaryError::Unavailable(tr("ai.download_local_short")));
    }

    #[test]
    fn every_other_error_is_the_crashed_message() {
        let crashed = SummaryError::Failed(tr("local_ai.crashed"));
        for e in [
            E::PromptTooLong { prompt_tokens: 9, n_ctx: 8, max_tokens: 1 },
            E::BadSchema { detail: "x".into() },
            E::NoChatTemplate,
            E::BadModel,
            E::Internal { message: "x".into() },
            E::VersionMismatch { helper: 1, app: 2 },
            E::BadRequest,
            E::Unsupported { detail: "x".into() },
            E::GpuFailed { fell_back: false },
        ] {
            assert_eq!(to_summary_error(e), crashed);
        }
    }

    #[test]
    fn resetting_gives_the_next_call_a_fresh_client() {
        // The statics are the process's: this is the only test that touches them.
        let dir = tempfile::tempdir().unwrap();
        let (a, s) = (summary_client(dir.path()), search_client(dir.path()));
        assert!(Arc::ptr_eq(&a, &summary_client(dir.path())), "the same client until a reset");
        reset_clients();
        let (b, t) = (summary_client(dir.path()), search_client(dir.path()));
        assert!(!Arc::ptr_eq(&a, &b) && !Arc::ptr_eq(&s, &t));
        assert!(Arc::ptr_eq(&b, &summary_client(dir.path())));
        reset_clients(); // nothing running: fine, and again with nothing built
        reset_clients();
    }

    #[test]
    fn the_helper_is_found_next_to_the_dev_binaries() {
        let dir = helper_dir();
        assert!(dir.file_name().is_some_and(|n| n != "deps"), "{dir:?}");
    }

    /// Real model through the real helper. Build it first (`sh scripts/build-llm-helper.sh`), then
    /// `TALAAN_LLM_MODEL=<gguf> cargo test --release local_ai_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_ai_smoke() {
        use crate::summarizer::engines::LocalEngine;
        use crate::summarizer::prompt::Prompt;
        use crate::summarizer::summarize::SummaryEngine;
        let model = std::env::var("TALAAN_LLM_MODEL").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ai = Arc::new(client(tmp.path(), "test", "summary"));
        let mut engine = LocalEngine::new(ai.clone(), PathBuf::from(model), "test".into(), true);
        let p = Prompt {
            system: "Reply with JSON only.".into(),
            user: "Give a title and a one-sentence recap for: 'We agreed to ship onboarding first and move pricing to Q1.'".into(),
        };
        let schema = r#"{"type":"object","properties":{"title":{"type":"string"},"recap":{"type":"string"}},"required":["title","recap"]}"#;
        let t = std::time::Instant::now();
        let text = engine.complete(&p, schema, 200, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        eprintln!("reply in {:.1}s ({} chars)", t.elapsed().as_secs_f32(), text.len());
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(v["recap"].as_str().is_some());
        ai.shutdown();
    }
}

/// Bake-off: the real prompt, schema and parser on the planning-call fixture, for every GGUF in a folder.
/// `TALAAN_LLM_DIR=<folder> cargo test --release bakeoff -- --ignored --nocapture`
#[cfg(test)]
mod bakeoff {
    use super::*;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::parse::{parse_draft, to_summary};
    use crate::summarizer::prompt::{summary_prompt, tests::meeting, SCHEMA};
    use std::time::Instant;
    use voice_engine_protocol::Completion;

    #[test]
    #[ignore]
    fn bakeoff() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/summary.planning-call.json")).unwrap();
        let lines: Vec<TranscriptLine> = serde_json::from_value(fixture["transcript"].clone()).unwrap();
        let mut m = meeting(lines.clone());
        m.speakers = serde_json::from_value(fixture["speakers"].clone()).unwrap();
        m.marks = serde_json::from_value(fixture["marks"].clone()).unwrap();
        m.item.duration_ms = fixture["durationMs"].as_u64().unwrap();
        let prompt = summary_prompt(&m, &lines, "en", None, &Default::default());
        let dir = std::env::var("TALAAN_LLM_DIR").unwrap();
        let mut models: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "gguf")).collect();
        models.sort();
        let tmp = tempfile::tempdir().unwrap();
        for path in models {
            let ai = client(tmp.path(), "test", "summary");
            let c = Completion {
                model_path: path.to_string_lossy().into(),
                n_ctx: 8192,
                system: prompt.system.clone(),
                user: prompt.user.clone(),
                json_schema: Some(SCHEMA.into()),
                max_tokens: 1500,
                temperature: 0.2,
                gpu: true,
                thinking: false,
                thinking_budget: None,
                seed: None,
            };
            let t = Instant::now();
            let r = ai.complete(c, &mut |_| {});
            let name = path.file_name().unwrap().to_string_lossy();
            match r {
                Err(e) => eprintln!("\n== {name}: ERROR {e:?}"),
                Ok(o) => {
                    let r = o.value;
                    eprintln!("\n== {name}: {:.1}s total on {}, {} prompt tok, {} out tok, truncated {}", t.elapsed().as_secs_f32(), o.backend.backend, r.prompt_tokens, r.output_tokens, r.truncated);
                    match parse_draft(&r.text) {
                        Err(e) => eprintln!("   PARSE ERROR: {e}"),
                        Ok(d) => {
                            let s = to_summary(&d, &lines, "x");
                            eprintln!("   title: {}\n   recap: {}", d.title, s.recap);
                            for x in &s.decisions { eprintln!("   D [{:?}] {}", x.at.map(|a| a / 1000), x.text); }
                            for x in &s.action_items { eprintln!("   A [{:?}] {} | owner {:?} | due {:?}", x.at.map(|a| a / 1000), x.text, x.owner, x.due); }
                            for x in &s.open_questions { eprintln!("   Q [{:?}] {}", x.at.map(|a| a / 1000), x.text); }
                        }
                    }
                }
            }
            ai.shutdown();
        }
    }
}
