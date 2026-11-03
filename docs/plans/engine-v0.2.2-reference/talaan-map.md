# Talaan -> shared engine crate: exact map

Repo: ~/Developer/Talaan, branch engine-v0.2.2 == origin/main == 69f0e91 "Release 1.1.1". Clean tree. Paths below are relative to the repo root unless they start with `/`. `S` = `src-tauri/src`. All line numbers verified on this checkout.

Totals: 668 Rust `#[test]` (about 22 `#[ignore]`d smoke/live tests), 38 Vitest files, 1 node test (`scripts/latest-json.test.mjs`).

---------------------------------------------------------------------------------------------------
## 0. Layout and cross-cutting facts that affect the plan

- Workspace: `src-tauri/Cargo.toml` `[workspace] members = [".", "llm", "llm-protocol", "stt"]` (line 24). One shared `src-tauri/Cargo.lock`. Package `talaan` v1.1.1, edition 2024, lib `talaan_lib` (staticlib, cdylib, rlib).
- Module wiring: `S/transcriber/mod.rs` declares pub mods: diarize, download, echo, fallback, engine, merge, models, pipeline, relocate, remote, vad, worker. `S/summarizer/mod.rs`: chunk, engines, followup, local, models, parse, prompt, summarize, templates, worker. `S/lib.rs:9` `mod helper;` (and `mod gpu;`).
- Everything that is a *model type* lives in `transcriber/models.rs` and is reused for LLM and embedding models (`EngineKind::Llm`, `EngineKind::Embedding`). `summarizer/models.rs` imports `transcriber::models::{EngineKind, ModelFile, ModelInfo}` (line 5). `eta.rs:8` imports `{EngineKind, ModelInfo}`.
- Model folder layout on disk: `<models_root>/<model id>/<file names from ModelInfo.files>`. `models_root` = `<data>/models` on Mac; Windows = `%LOCALAPPDATA%\<bundle id>\models` (set once via `set_models_root`, only when `TALAAN_DATA_DIR` unset).
- Data dir: Mac `~/Library/Application Support/Talaan`; override env `TALAAN_DATA_DIR` (`S/commands/mod.rs:60 resolve_data_dir`; `S/lib.rs:72,73,76,83`; `S/logging.rs:36 resolve_dir` -> `<TALAAN_DATA_DIR>/logs`).
- Tests that read other files via `include_str!` and will break if files move/change: `S/transcriber/models.rs:459` (reads `src/data/defaultSettings.ts`, checks every CATALOG id appears in the same ORDER as `id: "<id>"`), `models.rs:521,528` (tauri.conf.json `identifier`/`productName`; `windows/installer-hooks.nsh` must contain `RmDir /r "$LOCALAPPDATA\${BUNDLEID}\models"` and `RmDir /r "$APPDATA\Talaan"`), `S/logging.rs:938` (tauri.conf.json), `S/keys.rs:58` & `S/deeplink.rs:131` (installer-hooks.nsh), `S/platform/windows/notify.rs:199` (tauri.conf.json), `S/summarizer/local.rs:542` and `S/summarizer/summarize.rs:376` (fixtures/summary.planning-call.json), `S/summarizer/engines.rs:477` (src/data/byokModels.ts), `S/commands/mod.rs:78-91` (source scan of commands/*.rs: no sync `#[tauri::command]` may call a window builder; async commands need `#[tauri::command(async)]`/`async fn`).
- `S/log_scan.rs:350` (test-only privacy lint over log calls) walks `["src", "stt/src", "llm-protocol/src"]` relative to `src-tauri/`. If `stt/` or `llm-protocol/` are deleted that test panics on `read_dir(...).unwrap()`; the new engine crate's log calls are outside the scan unless the list is updated. FORBIDDEN field names include `path`, `file`, `dir`, `name`, `text`, `url`, `error`-ish generic names must go through `logging::text(..)`/`.for_log()`.
- `S/logging.rs:141` `const OWN: &[&str] = &["talaan_lib", "talaan", "talaan_stt"];` filter: `wanted(target, level)` (line 144) drops every log record whose crate name is not in OWN. A new engine crate (e.g. `talaan_engine`) emits NOTHING to talaan.log until added. Tests at `logging.rs:819-821` assert `wanted("talaan_stt::engine", Warn)` true and `talaan_stt_extra` false; doc comment line 13 and 141 name `talaan_stt`.
- `S/logging.rs` `logging::text(&str)` redacts; used by download/worker/gpu/commands (`crate::logging::text(e)`).
- i18n for engine errors: `DownloadError::user_message()` (`download.rs:32`) calls `crate::i18n::{tr, tr_err, tr_with}` with keys `error.cancelled`, `download.network{name,error}`, `download.corrupt{name}`, `download.save_failed{error}`, `download.stalled{name}`, `download.not_a_model{name}` (defined in `S/i18n.rs:152,379,380,381,413,419`; EN/FR/ES). `LocalAi::message` (`local.rs:198`) uses `local_ai.no_start`, `local_ai.too_slow`, `local_ai.crashed`, `local_ai.out_of_memory` (`i18n.rs:264-273`). An engine crate must return typed errors and keep the i18n mapping in Talaan.
- Other `Cargo.toml` deps NOT removable even after the swap: `sha2` (also `autosave.rs`, `ask/index.rs`, `commands/diagnostics.rs`), `reqwest` (logging, notion, calendar/fetch, summarizer/engines), `tiny_http` dev (notion, engines tests), `hound` (importer, recorder, worker/pipeline/vad), `tokio` (download.rs + commands/updater_e2e.rs), `thiserror` (also storage/mod.rs), `tempfile` dev (many).

---------------------------------------------------------------------------------------------------
## 1. `S/transcriber/models.rs` (532 lines)

### Pub API (exact)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind { Parakeet, ParakeetUnified, Nemotron, Whisper, Llm, Diarization, Embedding }
impl EngineKind { pub fn is_speech(self) -> bool }  // Parakeet|ParakeetUnified|Nemotron|Whisper

#[derive(Debug)] pub struct ModelFile { pub name: &'static str, pub url: &'static str, pub size: u64, pub sha256: &'static str }
#[derive(Debug)] pub struct ModelInfo {
  pub id: &'static str, pub name: &'static str, pub maker: &'static str, pub note: &'static str,
  pub engine: EngineKind, pub languages: &'static str, pub speed: u8 /*1..3*/, pub accuracy: u8 /*1..3*/,
  pub files: &'static [ModelFile],
  pub reuse_dirs: &'static [&'static str],   // relative to another app's data folder
}
impl ModelInfo { pub fn size(&self) -> u64 }   // sum of file sizes

pub static CATALOG: &[ModelInfo]   // line 68; speech models only (11 entries)
pub static VAD: ModelInfo          // line 244; id "vad", engine: EngineKind::Whisper (!), reuse_dirs ["Models"]
pub static DIARIZATION: ModelInfo  // line 260; id "diarization", engine Diarization, reuse_dirs []
pub static SEARCH: ModelInfo       // line 279; id "search", engine Embedding, reuse_dirs []
pub const DEFAULT_MODEL: &str = "whisper-turbo";   // line 297
pub fn find(id: &str) -> Option<&'static ModelInfo>   // VAD, DIARIZATION, SEARCH, then CATALOG (NOT the LLM catalog)

#[derive(Debug, Clone, PartialEq, Eq)] pub enum Location { Missing, Own(PathBuf), OnThisMac(PathBuf) }
impl Location { pub fn dir(&self) -> Option<&Path> }

static MODELS_ROOT: OnceLock<PathBuf>            // private (line 336)
pub fn set_models_root(root: PathBuf)             // #[cfg_attr(not(windows), allow(dead_code))]
pub fn local_models_root(local_app_data: &Path, identifier: &str) -> PathBuf   // <LOCALAPPDATA>/<id>/models
pub fn models_root(data_dir: &Path) -> PathBuf    // MODELS_ROOT or data_dir/models

#[derive(Debug, Clone)]
pub struct ModelStore { pub root: PathBuf, pub legacy_root: Option<PathBuf>, pub reuse_roots: Vec<PathBuf>, pub reuse: bool }
impl ModelStore {
  pub fn new(data_dir: &Path, reuse: bool) -> Self      // root = models_root(data_dir); legacy_root = data_dir/models if different; reuse_roots = default_reuse_roots()
  fn with_root(data_dir, root, reuse) -> Self           // private, test-only use
  pub fn own_dir(&self, m: &ModelInfo) -> PathBuf       // root/<id>
  pub fn own_dirs(&self, m: &ModelInfo) -> Vec<PathBuf> // [own_dir, legacy_root/<id>]
  pub fn locate(&self, m: &ModelInfo) -> Location       // own dirs first, then (if reuse) reuse_roots x reuse_dirs; "complete" = every file exists with EXACT size
}
fn complete(dir, m) -> bool            // private; size-only check (no hash)
fn default_reuse_roots() -> Vec<PathBuf>   // private: [dirs::data_dir()/com.craftconceptsdigital.parlato]
```
`complete`/`default_reuse_roots` are private. `DEFAULT_MODEL` is only used in `worker.rs` and the tests.

### Catalog data (ids, engine, files, size bytes, reuse_dirs)
All URLs are huggingface.co pinned by commit; every file has a sha256 (64 hex). Reuse root is `~/Library/Application Support/com.craftconceptsdigital.parlato` (Parlato app).
| id | EngineKind | files (size) | reuse_dirs |
|---|---|---|---|
| whisper-turbo | Whisper | ggml-large-v3-turbo-q5_0.bin 574_041_195 | ["Models"] |
| parakeet-v3 | Parakeet | encoder-model.int8.onnx 652_183_999; decoder_joint-model.int8.onnx 18_202_004; nemo128.onnx 139_764; vocab.txt 93_939; config.json 97 | ["ParakeetModels/parakeet-tdt-0.6b-v3-int8"] |
| whisper-small | Whisper | ggml-small.bin 487_601_967 | ["Models"] |
| whisper-large-v3 | Whisper | ggml-large-v3-q5_0.bin 1_081_140_203 | ["Models"] |
| whisper-large-v2 | Whisper | ggml-large-v2-q5_0.bin 1_080_732_091 | ["Models"] |
| whisper-medium | Whisper | ggml-medium.bin 1_533_763_059 | ["Models"] |
| whisper-base | Whisper | ggml-base.bin 147_951_465 | ["Models"] |
| whisper-tiny | Whisper | ggml-tiny.bin 77_691_713 | ["Models"] |
| parakeet-ultra | Parakeet | same 5 file names as v3 (encoder 649_524_002; decoder 18_203_490) | ["ParakeetModels/parakeet-ultra-0.6b-int8"] |
| parakeet-v2 | Parakeet | encoder 652_184_014; decoder 8_998_286; nemo128; vocab.txt 9_384; config.json | ["ParakeetModels/parakeet-tdt-0.6b-v2-int8"] |
| parakeet-unified-en | ParakeetUnified | encoder.int8.onnx 42_606_669; encoder.int8.onnx.data 611_491_584; decoder_joint.int8.onnx 8_995_064; tokenizer.model 251_056 | ["ParakeetModels/parakeet-unified-en-0.6b-int8"] |
| nemotron-3.5 | Nemotron | encoder.onnx 42_164_972; encoder.onnx.data 2_454_405_120; decoder_joint.onnx 97_590_054; tokenizer.model 406_554 | ["ParakeetModels/nemotron-3.5-asr-streaming-0.6b"] |
| vad (static VAD) | Whisper | ggml-silero-v6.2.0.bin 885_098 | ["Models"] |
| diarization (static) | Diarization | nemotron3_diar_v3.onnx 400_506_656 | [] |
| search (static) | Embedding | Qwen3-Embedding-0.6B-Q8_0.gguf 639_150_592 | [] |
LLM catalog (`S/summarizer/models.rs`, separate): `qwen3.5-4b` (Qwen3.5-4B-Q4_K_M.gguf 2_740_937_888) and `qwen3.5-9b` (Qwen3.5-9B-Q4_K_M.gguf 5_680_522_464), EngineKind::Llm, reuse_dirs [].
Per-model fields (name, maker, note, languages, speed, accuracy) are duplicated by hand in `src/data/defaultSettings.ts` (see section 8) and enforced by order-only test (`models.rs:459`).

### Call sites OUTSIDE `models.rs`
- `S/transcriber/worker.rs:6` `use super::models::{find, ModelInfo, ModelStore, DEFAULT_MODEL, DIARIZATION, VAD};`
  - 33 `pub model: &'static ModelInfo` (Config field); 44-46 `Config::from_settings`: `v["activeSpeechModel"].as_str().and_then(find).filter(|m| m.id != VAD.id).unwrap_or_else(|| find(DEFAULT_MODEL).unwrap())`
  - 257 `let mut store = ModelStore::new(&deps.data_dir, config.reuse);` 258-259 `store.reuse_roots = roots.clone()` (test hook `Deps.reuse_roots`, line 73)
  - 261 `store.locate(config.model).dir()` and `store.locate(&VAD).dir()`; 285 `vad_dir.join(VAD.files[0].name)`; 294 `store.locate(&DIARIZATION).dir()`; 296 `dir.join(DIARIZATION.files[0].name)`.
- `S/commands/transcriber.rs:7` `use crate::transcriber::models::{find, EngineKind, Location, ModelInfo, ModelStore, CATALOG, VAD};`
  - 53 `find(id).filter(|m| m.id != VAD.id).or_else(|| crate::summarizer::models::find_llm(id))` (`find_any`, the model resolver for download/cancel/delete)
  - 63 `model.engine != EngineKind::Whisper` (`stt_on_gpu`)
  - 167 `find(&model).filter(|m| m.engine.is_speech())` (meetings_retranscribe)
  - 170 `ModelStore::new(&t.data_dir, config.reuse).locate(m) == Location::Missing`
  - 212 `ModelStore::new(&t.data_dir, config.reuse)`; 215-217 `CATALOG.iter().chain([&crate::transcriber::models::DIARIZATION, &crate::transcriber::models::SEARCH])`; 219-224 `store.locate(m)` -> state strings; 230-236 `match m.engine {Parakeet|ParakeetUnified|Nemotron => "parakeet", Whisper => "whisper", Llm => "llm", Diarization => "diarization", Embedding => "search"}`; 237 `m.size().div_ceil(1_000_000)`; 243 `partial_fraction(m, &store.own_dir(m))`; 244 `was_cancelled(&store.own_dir(m))`
  - 271 `model.engine == crate::transcriber::models::EngineKind::Llm`; 281 `ModelStore::new(&t.data_dir, false).own_dir(model)`; 287 `ModelStore::new(&data_dir, false)`; 299 `model.engine.is_speech()`; 300 `store.locate(&VAD) == Location::Missing && ModelStore::new(&data_dir, true).locate(&VAD) == Location::Missing`; 303 `store.own_dir(&VAD)`; 306 `store.own_dir(model)`; 323 `model.engine == EngineKind::Embedding`; 340 `ModelStore::new(&t.data_dir, false).own_dir(m)`; 353 `ModelStore::new(&t.data_dir, false).own_dirs(model)`
- `S/commands/summarizer.rs:17` `use crate::transcriber::models::{Location, ModelStore};` 91 `ModelStore::new(data_dir, false).locate(model)` (pattern `Location::Own(dir)`; reuse is deliberately false for LLMs); 411 `ModelStore::new(t.data_dir(), false)`; 416 `store.locate(m)`; 418-419 `Location::Missing`; 421 `partial_fraction(m, &store.own_dir(m))`; 422 `was_cancelled(&store.own_dir(m))`
- `S/commands/ask.rs:14` `use crate::transcriber::models::{Location, ModelStore, SEARCH};` 83 `index::ensure(&dir, SEARCH.id, ...)`; 221 `index::ensure(dir, SEARCH.id, ps, &mut embed)`; 228-229 `match ModelStore::new(data_dir, false).locate(&SEARCH) { Location::Own(dir) => Some(dir.join(SEARCH.files[0].name)), _ => None }` (fn `search_model`, line 227)
- `S/eta.rs:8` `use crate::transcriber::models::{EngineKind, ModelInfo};` 51 `stt_profile(model: &ModelInfo, on_gpu: bool)`: 52 `model.engine == EngineKind::Whisper`, 55-61 matches on `model.id` ("whisper-tiny"/"whisper-base"/"whisper-small"/"whisper-medium"/"whisper-large-v2"|"whisper-large-v3"/"parakeet-ultra") and `EngineKind::{Whisper,Nemotron}`; 76 `Profile.key = "stt:{id}:{device}"`; test line 125 `use crate::transcriber::models::find`.
- `S/commands/settings.rs:40` and `:72` `crate::transcriber::models::models_root(storage.root())`.
- `S/lib.rs:85` `transcriber::models::local_models_root(&local, &app.config().identifier)`; `:86` `transcriber::models::set_models_root(new.clone())` (cfg windows).
- `S/summarizer/models.rs:5,7,...` builds `ModelInfo`/`ModelFile` literals; `find_llm`, `local_model` (see section 6).
- Test-only: `models.rs` own tests (7), `worker.rs` tests (14), `eta.rs` tests, `commands/ask.rs` tests.

---------------------------------------------------------------------------------------------------
## 2. `S/transcriber/download.rs` (551 lines, 14 tests, 1 ignored `real_download`)

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DownloadError { Cancelled, Network(String, String), Corrupt(String), Io(String), Stalled(String), NotAModel(String) }
// Display (English, for logs): "download cancelled" | "couldn't download {0}: {1}" | "{0} was damaged in transit, try again" | "couldn't save the model: {0}" | "the download of {0} stalled" | "got a web page instead of {0}"
impl DownloadError { pub fn user_message(&self) -> String }   // i18n via crate::i18n (see section 0)
impl From<std::io::Error> for DownloadError                  // -> Io
pub const IDLE: Duration = 30 s;                              // stall timeout
pub fn download_model(model: &ModelInfo, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<(), DownloadError>   // 0..1 over whole model, BLOCKING (reqwest blocking + own tokio rt)
pub fn download_model_with(model, dir, cancel, idle: Duration, progress) -> Result<(), DownloadError>   // tests use short idle
pub fn partial_fraction(model: &ModelInfo, dir: &Path) -> Option<f32>   // whole files + `<name>.part`; None unless a .part exists
pub fn mark_cancelled(dir: &Path)    // writes `<dir>/.cancelled` (survives relaunch)
pub fn clear_cancelled(dir: &Path)
pub fn was_cancelled(dir: &Path) -> bool
```
Behaviour to preserve: files downloaded one at a time to `<name>.part`, HTTP Range resume, size+sha256 verify then rename, web-page (HTML) rejected, 30 s idle stall keeps the `.part`, cancel checked every 200 ms (POLL), reqwest rustls-tls blocking.

Call sites (all in `S/commands/transcriber.rs` + `commands/summarizer.rs`):
- `commands/transcriber.rs:6` imports `{clear_cancelled, download_model, mark_cancelled, partial_fraction, was_cancelled, DownloadError}`
- 243 `partial_fraction(m, &store.own_dir(m))`; 244 `was_cancelled(&store.own_dir(m))`
- 281 `clear_cancelled(&ModelStore::new(&t.data_dir, false).own_dir(model));`
- 303 `result = download_model(&VAD, &store.own_dir(&VAD), &cancel, &mut |_| {});` (VAD first, silent, only for speech models when VAD missing)
- 306 `result = download_model(model, &store.own_dir(model), &cancel, &mut report);`
- 311 `if *e != DownloadError::Cancelled` (PartialEq needed); 312 `e.user_message()`; 318 `Err(DownloadError::Cancelled)`; 319 `crate::logging::text(e)` (Display); 326 `Err(DownloadError::Cancelled) => {}`; 328 `e.user_message()`
- 340 `mark_cancelled(&ModelStore::new(&t.data_dir, false).own_dir(m))`
- `commands/summarizer.rs:421` `crate::transcriber::download::partial_fraction(m, &store.own_dir(m))`; `:422` `crate::transcriber::download::was_cancelled(&store.own_dir(m))`
- Windows move uses none of these. No other callers.

---------------------------------------------------------------------------------------------------
## 3. `S/transcriber/relocate.rs` (446 lines, 11 tests; whole file `#![cfg_attr(not(windows), allow(dead_code))]`)
Windows-only feature (v1.1): move `%APPDATA%\Talaan\models` -> `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\models`. Pure std + one Win32 call (`free_bytes`, lines 174/184).
```rust
const PARTIAL: &str = ".talaan-move-";  const SPARE_BYTES: u64 = 1<<30;   // private
#[derive(Debug, Default, PartialEq)] pub struct Moved { pub renamed: bool, pub moved: usize, pub cleaned: usize, pub left: Vec<String> }
pub fn prepare(old: &Path, new: &Path) -> io::Result<Moved>                    // sync, before workers start
pub fn copy_rest(old: &Path, new: &Path, names: &[String], free: impl Fn(&Path) -> Option<u64>) -> Moved   // background
pub fn free_bytes(path: &Path) -> Option<u64>                                 // cfg(windows) Win32 / cfg(not) stub returns None
pub fn forget<T>(id: &str, then: impl FnOnce() -> T) -> T                      // Delete model coordination (global Mutex STATE + Condvar IDLE)
pub fn stop(timeout: Duration)                                                // Delete everything: stop copy, wait
#[cfg(test)] pub(crate) fn reset_for_tests()
```
Call sites:
- `S/lib.rs:88` (cfg windows) `transcriber::relocate::prepare(&old, &new)` -> `Ok(m) => m.left`; Err logs `e.kind()` only (path redaction). `S/lib.rs:139` `transcriber::relocate::copy_rest(&old, &new, &left, transcriber::relocate::free_bytes)` (thread spawned after setup, lines ~136-146), uses `r.left`, `r.moved`; then `:144` `commands::transcriber::models_moved(&handle)`.
- `S/commands/transcriber.rs:354` `crate::transcriber::relocate::forget(model.id, || -> CmdResult<()> { remove_dir_all each own_dirs })` inside `models_delete`.
- `S/commands/settings.rs:36` `crate::transcriber::relocate::stop(std::time::Duration::from_secs(10));` (data_delete_all).
- `S/transcriber/models.rs:504` test `the_old_folder_is_read_until_the_move_is_done` calls `crate::transcriber::relocate::prepare(&data.path().join("models"), &store.root)`.
- `ModelStore.legacy_root` exists only to serve this.

---------------------------------------------------------------------------------------------------
## 4. Speech runtimes: engine.rs / vad.rs / diarize.rs / pipeline.rs / merge.rs / echo.rs / worker.rs / remote.rs / fallback.rs

### 4a. `engine.rs` (300 lines; 3 ignored smoke tests: engine_smoke, + two at 235/274)
```rust
#[derive(Debug, Clone, PartialEq)] pub struct Piece { pub start_ms: u64, pub end_ms: u64, pub text: String }
#[derive(Debug, Clone, Default)]  pub struct ChunkResult { pub pieces: Vec<Piece>, pub language: Option<String> }
pub trait Engine: Send {
  fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<ChunkResult, String>;  // 16 kHz mono chunk
  fn label(&self) -> &str;
  fn chunking(&self) -> super::vad::Shape;
}
pub fn load(model: &ModelInfo, dir: &Path, use_gpu: bool) -> Result<Box<dyn Engine>, String>   // dispatch on model.engine; Llm|Diarization|Embedding -> Err("not a speech model")
pub use talaan_stt::is_noise;
pub struct ParakeetEngine { model: NvidiaModel{Tdt|Unified|Nemotron}, label }  impl ParakeetEngine { pub fn load(dir:&Path, label:&str, kind:EngineKind)->Result<Self,String> }
pub struct WhisperEngine { state: WhisperState, _ctx: WhisperContext, label }  impl WhisperEngine { pub fn load(file:&Path, label:&str, use_gpu:bool)->Result<Self,String> }
```
- MODEL RUNTIME (candidates for the engine crate): `engine::load` dispatch, `NvidiaModel`, `ParakeetEngine::load` (parakeet_rs ExecutionConfig intra threads, `ParakeetTDT/ParakeetUnified::from_pretrained(dir)`, `NemotronHandle::from_pretrained` + `Nemotron::from_shared` + `available_languages()`), `ParakeetEngine::set_language` (Nemotron "fr" -> "fr-FR" prompt, `set_target_lang`), `ParakeetEngine::run` (8000-sample zero pad each side, `TimestampMode::Sentences`, 500 ms shift, `is_noise` filter), `WhisperEngine::load` (`talaan_stt::context_params(use_gpu.then_some(0), cfg!(macos) && use_gpu)`; flash-attn only Metal) and `WhisperEngine::transcribe` -> `talaan_stt::transcribe(&mut state, samples, language, threads())`, `threads()` -> `talaan_stt::threads()`.
- TALAAN PIPELINE LOGIC inside engine.rs (decide whether it moves): the Parakeet "resume if decoding stopped early" loop in `impl Engine for ParakeetEngine::transcribe` (max 8 iterations, `RESUME_GAP_MS = 2_000`, progressed > 500 ms) and each engine's `chunking()` (-> `vad::Shape::parakeet()` / `Shape::whisper()`), `label()`.
- Call sites outside engine.rs: `S/transcriber/fallback.rs:88,91,95` (`engine::load(model, dir, use_gpu)` and `engine::load(cpu_model, &cpu_dir, false)`), `S/transcriber/worker.rs:4` `use super::engine::Engine;` (type of `Deps.load_engine` and `Loaded.engine`), `S/transcriber/pipeline.rs:6,216,350`, `remote.rs:5` (`Engine, ChunkResult, Piece`), tests in worker.rs:396 / fallback.rs:101. No call from commands/ except via fallback.

### 4b. `vad.rs` (270 lines; 7 tests, 1 ignored vad_smoke)
```rust
pub const RATE: usize = 16_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Span { pub start: usize, pub end: usize }  // impl len(), start_ms()
#[derive(Debug, Clone, Copy)] pub struct Shape { pub pad, pub join_gap, pub max_len, pub min_len, pub pack_gap: usize }  // Default; Shape::parakeet(); Shape::whisper()
pub fn shape(raw: &[Span], samples: &[f32], s: Shape) -> Vec<Span>      // pad, join, split at quietest point, drop short, pack_gap packing
pub fn read_track(path: &Path) -> std::io::Result<Vec<f32>>             // WAV 16 kHz mono i16 -> f32 (tolerates bad header)
pub struct Vad { ctx: whisper_rs::WhisperVadContext }
impl Vad { pub fn load(model: &Path) -> Result<Self, String>; pub fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String> }
```
- MODEL RUNTIME: `Vad::load` (whisper_rs `WhisperVadContextParams`, n_threads 4, use_gpu false) and `Vad::detect` (threshold 0.5, min_speech 250 ms, min_silence 300 ms, speech_pad 0, centiseconds -> samples).
- TALAAN PIPELINE LOGIC: `Span`, `Shape` (+ presets), `shape`, `pack`, `split`, `read_track`/`data_offset`, `RATE`. (`Shape` is part of the `Engine::chunking()` trait signature, so it is coupled to the Engine trait.)
- Call sites outside: `commands/transcriber.rs:9,89` `use crate::transcriber::vad::Vad;` `load_detector: Box::new(|path| Vad::load(path).map(|v| Box::new(v) as Box<dyn Detector + Send>))`; `worker.rs` via `Detector`; `pipeline.rs:9` `use super::vad::{read_track, shape, Span, Vad};` and `impl Detector for Vad` (pipeline.rs:18); `engine.rs/remote.rs/fallback.rs` use `Shape`.

### 4c. `diarize.rs` (168 lines; 5 tests, 1 ignored diarize_smoke)
```rust
pub const RATE: usize = 16_000;                 // WARMUP = 10 min, CHUNK = 5 min, MIN_ACTIVITY = 0.2 (private)
pub type Activity = Vec<Vec<f32>>;              // frames(10 ms) x speakers
pub trait SpeakerSplitter: Send { fn activity(&mut self, them: &[f32]) -> Result<Activity, String>; }
pub struct Diarizer { model: parakeet_rs::sortformer::Sortformer }
impl Diarizer { pub fn load(path: &Path) -> Result<Self, String> }     // Sortformer::new(path)
impl SpeakerSplitter for Diarizer                                      // reset_state, warm-up on first 10 min (discarded), then 5-min chunks via diarize_chunk_raw
pub fn assign_speakers(pieces: &mut [Timed], activity: &Activity) -> usize   // Talaan logic
fn dominant(...)                                                        // private Talaan logic
```
- MODEL RUNTIME: `Diarizer::load`, `Diarizer::activity` (incl. warm-up/chunk constants, memory-flat chunking), the `SpeakerSplitter` trait (model-agnostic seam).
- TALAAN PIPELINE LOGIC: `assign_speakers`, `dominant` (numbering 1,2,3 in order of first appearance; <2 speakers -> all stay 0 "Them"; unknown sentence takes previous speaker), `Activity` type is shared interface.
- Call sites outside: `commands/transcriber.rs:90` `Box::new(|path| crate::transcriber::diarize::Diarizer::load(path).map(|d| Box::new(d) as Box<dyn crate::transcriber::diarize::SpeakerSplitter>))`; `worker.rs:5` `use super::diarize::SpeakerSplitter;`; `pipeline.rs:7` `use super::diarize::{assign_speakers, SpeakerSplitter};`.

### 4d. `pipeline.rs` (364 lines; 7 tests, 1 ignored pipeline_smoke) = all TALAAN logic
```rust
pub trait Detector { fn detect(&mut self, samples: &[f32]) -> Result<Vec<Span>, String>; }   // impl for Vad
#[derive(Debug, Clone, PartialEq)] pub struct Output { pub lines: Vec<TranscriptLine>, pub language: Option<String>, pub engine: String, pub echo_removed: usize, pub speech_ms: u64, pub speakers: usize }
#[derive(Debug, PartialEq)] pub enum PipelineError { Cancelled, Failed(String) }   // From<String>
pub fn run(dir:&Path, engine:&mut dyn Engine, detector:&mut dyn Detector, language:Option<&str>, splitter:Option<&mut dyn SpeakerSplitter>, cancel:&AtomicBool, progress:&mut dyn FnMut(f32)) -> Result<Output, PipelineError>
pub fn detect_language(lines: &[TranscriptLine]) -> Option<String>   // whatlang + iso639_1 (private)
```
Reads `me.wav`/`them.wav` from the meeting dir, VAD, shape, transcribe chunk by chunk (language settled on the longest chunk), re-transcribes uncovered speech >= 1 s (`uncovered`, MIN 16_000, CONTEXT 8_000), echo removal, speaker assignment, merge, language detection. Depends on `crate::storage::model::TranscriptLine`. Only call: `S/transcriber/worker.rs:7` (`use super::pipeline::{run, Detector, PipelineError}`); `commands/transcriber.rs:8` imports `pipeline::Detector` (line 89 cast). Test calls `engine::load` (pipeline.rs:350).

### 4e. `merge.rs` (114), `echo.rs` (143) = all TALAAN logic
`merge::{Track{Me,Them}, Timed{track,speaker:u8,start_ms,end_ms,text}, merge(Vec<Timed>)->Vec<TranscriptLine>}` (JOIN_GAP_MS 1_200, MAX_LINE_MS 40_000). `echo::{words(&str)->Vec<String>, remove_echo(Vec<Timed>)->(Vec<Timed>, usize)}` (strsim; WINDOW 2 s, MIN_SHARE 0.6, WORD_SIMILARITY 0.8). External call: `S/ask/search.rs:3` `use crate::transcriber::echo::words;`. Everything else internal (`pipeline.rs`, `diarize.rs` uses `merge::{Timed, Track}`).

### 4f. `worker.rs` (718 lines; 14 tests) = Talaan queue/storage glue
```rust
pub const NO_MODEL: &str = "no_model";  pub const WAITING: &str = "waiting";
pub struct Config { pub model: &'static ModelInfo, pub language: Option<String>, pub use_gpu: bool, pub reuse: bool, pub separate_speakers: bool, pub summarize: bool }
impl Config { pub fn from_settings(v: Option<&Value>) -> Config }  // keys: activeSpeechModel, meetingLanguage ("" or "auto"=detect), useGpu (def true), reuseModels (def true), separateSpeakers (def true), summarizeAutomatically (def true)
pub type EngineLoader   = dyn Fn(&ModelInfo, &Path, bool) -> Result<Box<dyn Engine>, String> + Send + Sync;
pub type DetectorLoader = dyn Fn(&Path) -> Result<Box<dyn Detector + Send>, String> + Send + Sync;
pub type SplitterLoader = dyn Fn(&Path) -> Result<Box<dyn SpeakerSplitter>, String> + Send + Sync;
pub type OnGpu = dyn Fn(&ModelInfo, bool) -> bool + Send + Sync;
pub struct Deps { pub storage: Arc<Mutex<Storage>>, pub data_dir: PathBuf, pub changed: Arc<dyn Fn()+Send+Sync>, pub load_engine: Box<EngineLoader>, pub load_detector: Box<DetectorLoader>, pub load_splitter: Box<SplitterLoader>, pub reuse_roots: Option<Vec<PathBuf>>, pub transcribed: Arc<dyn Fn(&str)+Send+Sync>, pub transcript_ready: Arc<dyn Fn(&str,&str)+Send+Sync>, pub on_gpu: Arc<OnGpu> }
pub struct Worker; impl Worker { pub fn start(deps: Deps) -> Worker; pub fn enqueue(&self,&str); pub fn cancel_if_running(&self,&str); pub fn idle(&self)->bool }
pub fn stamp_estimate(s:&Storage, id:&str, on_gpu:&OnGpu);  pub fn restamp_queue(s:&Storage, ids:&[String], on_gpu:&OnGpu);
```
Private consts: IDLE_UNLOAD 300 s (engine/detector/splitter unloaded after idle), MIN_LEARN_SPEECH_MS 60_000 (rate learning). The loader closures are already an injection seam: tests inject fake engines; the engine crate can plug in there. Error text hardcoded line 262: "Download a speech model in Settings › Speech models to transcribe this call." (code NO_MODEL).
External call sites: `commands/transcriber.rs:10` `use crate::transcriber::worker::{restamp_queue, stamp_estimate, Config, Deps, OnGpu, Worker, NO_MODEL};` -> 36-38 `stamp_estimate(storage, id, self.on_gpu.as_ref())`; 76-79 `on_gpu: Arc<OnGpu>` built from `stt_on_gpu`; 80-101 `Worker::start(Deps{...})` (load_engine at 85-88 = `fallback::load(m, dir, use_gpu, &gpu)`; load_detector 89; load_splitter 90; reuse_roots None 91; transcribed 92-95 -> `SummarizerState::enqueue_auto`; transcript_ready 96-99 -> `summarizer::notify_done(.., "notify.transcript_ready", title)`); 105 `restamp_queue(&storage, &waiting, on_gpu.as_ref())`; 107 `worker.enqueue(id)`; 129 `e.code == NO_MODEL`; 169 `Config::from_settings(storage.settings_get()?.as_ref())`, 211 same. Other users of `TranscriberState` (enqueue/stamp/worker): `commands/meetings.rs:62-63` (`t.worker.cancel_if_running(&id)`), `commands/import.rs:77`, `commands/recorder.rs:280-281,290,374`.

### 4g. `remote.rs` (247 lines; 5 tests) - Windows GPU Whisper helper client
```rust
pub const HELPER: &str = "talaan-stt-vulkan";   // used by gpu.rs:22 STT_HELPER
pub enum LoadError { NoGpu, Failed(String) }     // Debug, Clone, PartialEq
pub struct Timeouts { pub start: Duration, pub load: Duration, pub chunk: Duration, pub per_audio: u32 }  // Default: 60 s / 180 s / 60 s / x2
pub struct RemoteWhisper { proc: Option<Process<SttEvent>>, label, backend, timeouts }
impl RemoteWhisper { pub fn load(exe:&Path, env:&[(String,String)], model_file:&Path, label:&str, timeouts:Timeouts) -> Result<Self, LoadError> }  // Ping -> Ready{backend}; Load{model_path, gpu:true} -> Loaded
impl Engine for RemoteWhisper                    // Transcribe{samples: base64 f32, language}; errors become plain String
```
Imports `crate::helper::Process`, `talaan_llm_protocol::stt::{encode_samples, SttEvent, SttRequest}`, `talaan_llm_protocol::Backend`. External call: `fallback.rs:88` `RemoteWhisper::load(&exe, &env, &file, model.name, Timeouts::default())`; `gpu.rs:22`.

### 4h. `fallback.rs` (335 lines; 7 tests, 1 ignored gpu_whisper_live)
```rust
pub type CpuLoader = Box<dyn FnMut() -> Result<Box<dyn Engine>, String> + Send>;
pub type OnFailure = Arc<dyn Fn(&str) + Send + Sync>;
pub struct Fallback; impl Fallback { pub fn start(gpu: Result<Box<dyn Engine>, LoadError>, load_cpu: CpuLoader, on_failure: OnFailure, label:&str, chunking: Shape) -> Result<Box<dyn Engine>, String> }
pub fn load(model: &ModelInfo, dir: &Path, use_gpu: bool, gpu: &Arc<Gpu>) -> Result<Box<dyn Engine>, String>
```
`load`: only `cfg!(windows) && model.engine == EngineKind::Whisper` and `gpu.plan(Kind::Stt, use_gpu)` is `Plan::Gpu{exe, env}`: RemoteWhisper then Fallback to `engine::load(.., false)`; failures recorded via `gpu.recorder(Kind::Stt)` (writes gpu.json); else `engine::load`. Only external call: `commands/transcriber.rs:87`.

---------------------------------------------------------------------------------------------------
## 5. Local AI helper: `S/helper.rs`, `S/gpu.rs`, `S/commands/gpu.rs`, `S/summarizer/local.rs`

### 5a. `helper.rs` (440 lines; 5 tests; `pub mod fake` is `#[cfg(test)]`-ish test helper at line 281)
```rust
pub struct Process<E> { child, stdin, events: Receiver<E>, stderr: Arc<Mutex<StderrTail>>, stderr_reader }
pub struct Death { pub detail: String, pub out_of_memory: bool }
impl<E: DeserializeOwned + Send + 'static> Process<E> {
  pub fn spawn(exe: &Path, env: &[(String,String)]) -> std::io::Result<Self>   // hidden window on Windows (hide_console)
  pub fn send<R: Serialize>(&mut self, req: &R) -> std::io::Result<()>
  pub fn recv_timeout(&self, d: Duration) -> Result<E, RecvTimeoutError>
  pub fn alive(&mut self) -> bool;  pub fn died(self, who: &str) -> Death;  pub fn kill(self)
}
pub struct StderrTail; impl push(&str), text() -> String;  pub const TAIL_BYTES: usize = 2048; pub const LINE_BYTES: usize = 200;
pub fn ran_out_of_memory(status: Option<ExitStatus>, stderr: &str) -> bool   // MEMORY_MARKERS list, STATUS_NO_MEMORY 0xC000_0017, SIGKILL(9)
pub fn hide_console(cmd: &mut Command)
pub mod fake { Step{stdout,stderr,exit}, reply(&[&str]), crash(&[&str], i32), script(dir,name,&[Step]) -> PathBuf }   // shell-script fake helpers for tests
```
External uses: `S/commands/gpu.rs:6,78,82,86,95,169` (`Process::<Event>::spawn/send/recv_timeout/died/kill`, `fake::{reply, script}`); `S/summarizer/local.rs:10,347,407` (`Process`, `Death`, `fake::{crash, reply, script, Step}`); `S/transcriber/remote.rs:6`. Protocol coupling: the generic `E` is `talaan_llm_protocol::Event` / `stt::SttEvent`. Message text parsed: stderr markers; `died()` logs with `crate::logging::text`.

### 5b. `gpu.rs` (356 lines; 6 tests, 1 ignored gpu_here) - Windows Vulkan decision + `gpu.json`
```rust
pub const LLM_HELPER: &str = "talaan-llm-vulkan";  pub const STT_HELPER: &str = crate::transcriber::remote::HELPER;  // "talaan-stt-vulkan"
const FILE: &str = "gpu.json";                       // in the data dir
pub enum Kind { Llm, Stt }  // Debug Clone Copy PartialEq Eq PartialOrd Ord; fn key() "llm"/"stt"; pub fn helper(self)->&'static str
pub enum Skip { Off, NoVulkan, NoHelper, Failed(String) }
pub enum Plan { Gpu{exe: PathBuf, env: Vec<(String,String)>}, Cpu(Skip) }
#[serde(rename_all="camelCase")] pub struct Marker { pub app_version: String, pub driver: String, #[serde(default)] pub failures: BTreeMap<String, Failure> }
#[serde(rename_all="camelCase")] pub struct Failure { pub reason: String, pub at: String }   // REASON_BYTES 600
pub struct Gpu { data_dir, app_version, driver, helper_dir, loader }   // Clone
impl Gpu { pub fn detect(&Path)->Arc<Gpu>; pub fn shared(&Path)->Arc<Gpu> /*OnceLock*/; pub fn helper_path(&self,Kind)->PathBuf; pub fn plan(&self,Kind,use_gpu:bool)->Plan; pub fn marker(&self)->Option<Marker>; pub fn failure(&self,Kind)->Option<Failure>; pub fn record(&self,Kind,&str); pub fn clear(&self); pub fn recorder(self:&Arc<Self>,Kind)->Arc<dyn Fn(&str)+Send+Sync>
  #[cfg(test)] detect_with_helpers, fake }
pub fn describe(plan: &Plan) -> String;  pub fn helper_env() -> Vec<(String,String)> /* VK_LOADER_LAYERS_DISABLE=~implicit~ */;  pub fn fingerprint(Vec<(String,String)>)->String;  pub fn driver_fingerprint()->String /*winreg*/;  pub fn vulkan_loader_present()->bool /*windows crate LoadLibraryExW vulkan-1.dll*/
```
`plan()` conditions: use_gpu, `vulkan-1.dll` present, helper exe next to talaan.exe, no remembered failure for this app_version+driver. Mac never uses helpers (Metal in process).
External call sites: `commands/transcriber.rs:62-69` (`crate::gpu::Gpu`, `crate::gpu::Kind::Stt`, `crate::gpu::Plan::Gpu{..}`), `:77,:86` `crate::gpu::Gpu::shared(&data_dir)`; `commands/summarizer.rs:9` `use crate::gpu::{Gpu, Kind, Plan};` 104 `Gpu::shared(data_dir)`, 105 `gpu.plan(Kind::Llm, use_gpu)`, 106 `gpu.recorder(Kind::Llm)`, 130 `matches!(gpu.plan(Kind::Llm, use_gpu), Plan::Gpu { .. })`, 141 `Gpu::shared(&data_dir)`, 302 `summary_profile(&cfg, &Gpu::shared(&s.data_dir))`; `commands/gpu.rs:5,105-106,119`; `commands/diagnostics.rs:8` `use crate::gpu::{describe, Gpu, Kind};` 257 `crate::gpu::driver_fingerprint()`, 258-262 `describe(&gpu.plan(Kind::Stt|Llm, use_gpu))`, `Gpu::shared(data_dir)` 256.

### 5c. `commands/gpu.rs` (177 lines; 4 tests) - Check setup > Graphics chip
```rust
#[serde(rename_all="camelCase")] #[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct GpuStatus { pub state: &'static str /* "metal"|"in_use"|"off"|"unavailable"|"failed" */, pub reason: Option<&'static str> /* "no_vulkan"|"no_helper"|"no_gpu"|"no_start" */, pub device: Option<String>, pub vram_mb: Option<u64>, pub detail: Option<String> }  // Option fields skip_serializing_if None
pub enum Probe { Ready(Backend), Failed(String) }
pub fn status(mac: bool, use_gpu: bool, llm: Plan, stt: Plan, probe: impl FnOnce(&Plan)->Probe) -> GpuStatus   // pure, tested
fn probe_helper(plan:&Plan)->Probe   // spawns the LLM vulkan helper (Process::<Event>), Ping, 30 s timeout, kills it
#[tauri::command] pub async fn gpu_status(app: AppHandle) -> GpuStatus      // spawn_blocking
#[tauri::command] pub async fn gpu_retry(app: AppHandle) -> GpuStatus       // Gpu::shared(..).clear() then status
```
Reads settings `useGpu` (default true) at line 104. Uses `TranscriberState::data_dir()` (line 105, 119). Registered `S/lib.rs:247-248` (`commands::gpu::gpu_status`, `commands::gpu::gpu_retry`). Frontend type `GpuStatus` in `src/data/settings.ts:42-51`.

### 5d. `summarizer/local.rs` (582 lines; 12 tests, 2 ignored: local_ai_smoke @507, bakeoff @540) - the LLM/embedding client
```rust
pub const HELPER: &str = "talaan-llm";
pub fn helper_file() -> String            // "talaan-llm" + EXE_SUFFIX
pub fn helper_path() -> PathBuf           // current_exe().parent()/talaan-llm[.exe]  (Mac bundle: Contents/MacOS/; dev: target/<profile>/)
pub struct GpuHelper { pub exe: PathBuf, pub env: Vec<(String,String)>, pub on_failure: Arc<dyn Fn(&str)+Send+Sync>, pub stall: Duration }
pub const GPU_STALL: Duration = 5 min;
pub struct LocalAi { exe, running: Option<Process<Event>>, gpu: Option<Gpu{cfg,proc,off,backend}>, last_detail: Option<String> }
impl LocalAi {
  pub fn new(exe: &Path) -> Self;  pub fn with_gpu(exe: &Path, gpu: GpuHelper) -> Self;
  pub fn take_detail(&mut self) -> Option<String>;
  pub fn complete(&mut self, c: &Completion, cancel: &AtomicBool, progress: &mut dyn FnMut(Phase,u32,u32)) -> Result<Reply, String>;
  pub fn embed(&mut self, model_path: &Path, texts: Vec<String>, cancel: &AtomicBool) -> Result<Vec<Vec<f32>>, String>;   // always CPU helper, gpu:false
  pub fn stop(&mut self);   // + Drop calls stop()
}
```
Behaviour: Ping/Ready handshake (60 s), one request at a time, 250 ms poll, 30 min hard cap (`TooSlow`), stall limit for GPU helper only, GPU failure -> same request re-run on CPU helper and `on_failure(reason)` called only if the CPU then succeeds; helper kept alive after an `Event::Error` (`RunError::Said`). Private `RunError {Cancelled, Said, NoSpawn, NoStart(Option<Death>), TooSlow, Stalled, Died, Io}`; `message()` maps to i18n (`local_ai.*`).
**String-matched error contract** (must stay identical or callers change):
- cancellation is the literal string `"cancelled"` (local.rs:117,201; the helper-level `cancelled` also from `describe`). Consumer: `S/summarizer/engines.rs:129` `Err(e) if e == "cancelled" => Err(EngineError::Cancelled)`, any other string -> `EngineError::Failed(e)` (line 130). No other string matching exists; helper stderr text is classified inside `helper.rs::ran_out_of_memory`.
- embed error literal: `"the search model stopped unexpectedly"` (local.rs:227, log-only English). `ask/index.rs:~72` has its own `"the search model returned the wrong number of vectors"`; `commands/ask.rs:223` `"no vector for the question"`.
- `commands/summarizer.rs:151-152` `gpu_fault(reason)` = `!reason.to_lowercase().contains("memory")` (substring match on the GPU failure text; test at l.491 asserts "the GPU helper stopped answering" -> fault, "ErrorDeviceLost" -> fault, "not enough memory for the AI model: ErrorOutOfDeviceMemory" -> not a fault). Used only for follow-up drafts (`engine_for(.., followup=true)`).
- Summary worker maps `EngineError::{Unavailable(m)->NO_ENGINE, Failed(m)->FAILED, Cancelled}` (`summarizer/worker.rs:294-296,353-355`; test at 536 expects `"The local AI stopped unexpectedly. Try again."` = EN of `local_ai.crashed`).

Request shapes (all in `talaan-llm-protocol`, section 7): `Request::Complete(Completion{model_path,n_ctx,system,user,json_schema:Some(schema),max_tokens,temperature:0.2,gpu:true})` built in `S/summarizer/engines.rs:117-125` (`LocalEngine::complete`; `n_ctx = LOCAL_CONTEXT as u32`; `gpu` set true there, overwritten by `LocalAi` per helper); `Request::Embed(Embedding{model_path,texts,gpu:false})` in `LocalAi::embed`; `Request::Ping` in `start`.

Call sites of `LocalAi`/`local::*`:
- `S/summarizer/engines.rs:4` `use super::local::LocalAi;` :12 `use talaan_llm_protocol::Completion;` :108-141 `pub struct LocalEngine { pub ai: LocalAi, pub model_path: PathBuf, pub model_name: String }`; `impl SummaryEngine for LocalEngine { complete(), context_tokens() -> LOCAL_CONTEXT, label() -> "Local AI · {model_name}", failure_detail() -> self.ai.take_detail() }`; `self.ai.complete(&c, cancel, &mut |_, _, _| {})` at :127 (progress ignored).
- `S/commands/summarizer.rs:10` `use crate::summarizer::local::{helper_path, GpuHelper, LocalAi, GPU_STALL};` :91-95 `ModelStore::new(data_dir,false).locate(model)` -> `Ok(Box::new(LocalEngine { ai: local_ai(data_dir, s["useGpu"].as_bool().unwrap_or(true), followup), model_path: dir.join(model.files[0].name), model_name: model.name.into() }))`, else `EngineError::Unavailable(tr_with("ai.download_local", &[("name", model.name), ("size", &size)]))` (size = `model.size()/1e9` via `crate::i18n::decimal`); :102-112 `fn local_ai(data_dir:&Path, use_gpu:bool, followup:bool) -> LocalAi` (cfg!(windows) + `Plan::Gpu{exe,env}` -> `LocalAi::with_gpu(&helper_path(), GpuHelper{exe, env, on_failure, stall: GPU_STALL})` else `LocalAi::new(&helper_path())`).
- `S/commands/ask.rs:11` `use crate::summarizer::local::{helper_path, LocalAi};` :31 `embedder: Arc<Mutex<LocalAi>>`; :39 `LocalAi::new(&helper_path())`; :66 `index_all(app, data_dir, embedder: &Mutex<LocalAi>)`; :83 `ai.embed(&model, texts, &cancel)` inside `index::ensure(&dir, SEARCH.id, &ps, &mut |texts| ai.embed(&model, texts, &cancel))` (an `Err` aborts the index pass silently); :149 `answer_from(.., embedder: &Mutex<LocalAi>, ..)`; :157 `embed_all(..).ok().flatten()` (any embed error => keyword search fallback); :215-224 `embed_all` (`ai.embed(&model, texts, cancel)`, question embedded via `query_text(question)`); :280-281 test `LocalAi::new(&exe)` with `crate::summarizer::local::helper_file()`.
- Ask answers use the summary engine: `commands/ask.rs:~166` `crate::commands::summarizer::make_engine(&config, data_dir)` then `engine.complete(&prompt, answer::SCHEMA, REPLY_TOKENS, &cancel)` (:200) with `EngineError` matching at :169-171,205-206 (`Unavailable` wrapped in `ask.needs_ai`).
- `S/ask/mod.rs:26-27,36,39` test (ignored embed_smoke): `LocalAi::new(&exe)`, `ai.embed(..)`; `S/summarizer/summarize.rs:370,385-387` test: `LocalAi::new(&exe)`, `LocalEngine{..}`; `S/summarizer/local.rs` own tests.
- Summary/follow-up consumers of `SummaryEngine::complete`: `summarizer/summarize.rs:146,157`, `summarizer/followup.rs:113,118`.

### 5e. `S/summarizer/models.rs` (73 lines, 1 test)
```rust
pub static LLM_CATALOG: &[ModelInfo]            // [qwen3.5-4b, qwen3.5-9b]
pub fn local_model(setting: &str) -> &'static ModelInfo   // "larger" -> 9b; anything else -> 4b (settings localModel is "recommended"|"larger")
pub fn find_llm(id: &str) -> Option<&'static ModelInfo>
```
Callers: `commands/summarizer.rs:11` (`local_model`, `LLM_CATALOG`), :90,:128 `local_model(s["localModel"].as_str().unwrap_or("recommended"))`, :412 `LLM_CATALOG.iter().zip(["recommended","larger"])`; `commands/transcriber.rs:53` `find_llm(id)`; `eta.rs` reads `model.contains("9b")` by id string (line ~76-79).

---------------------------------------------------------------------------------------------------
## 6. Tauri commands, startup, logging (the app-side glue)

### 6a. `S/commands/transcriber.rs` (364 lines) - registered at `S/lib.rs:234-239`
Pub items: `pub const MODELS_CHANGED: &str = "models-changed"` (l.19); `pub struct TranscriberState { pub worker: Worker, data_dir: PathBuf, on_gpu: Arc<OnGpu>, downloads: Mutex<HashMap<String,(Arc<AtomicBool>, f32)>>, failed: Mutex<HashMap<String,String>> }` with `enqueue(&self,&str)`, `stamp_estimate(&self,&Storage,&str)`, `data_dir(&self)->&Path`, `download_progress(&self,&str)->Option<f32>`, `download_error(&self,&str)->Option<String>`; `pub fn setup(app:&AppHandle, data_dir:PathBuf)` (l.73; called `lib.rs:126`); `pub fn models_moved(app:&AppHandle)` (l.116; cfg windows use); `pub const DOWNLOAD_FAILED: &str = "model-download-failed"` (l.253).
Commands (all SYNCHRONOUS `#[tauri::command]`, so they must not build windows):
```rust
meetings_retry(app, state: State<AppState>, t: State<TranscriberState>, id: String) -> CmdResult<()>            // 146
meetings_retranscribe(app, state, t, id: String, model: String) -> CmdResult<()>                              // 166; errors: "No speech model called {model}" (hardcoded English), tr_with("error.download_speech_model",{name})
models_list(state: State<AppState>, t: State<TranscriberState>) -> CmdResult<Vec<ModelRow>>                  // 210 (speech catalog + DIARIZATION + SEARCH; NOT LLM)
models_download(app: AppHandle, t: State<TranscriberState>, id: String) -> CmdResult<()>                      // 269; spawns std::thread; also accepts LLM ids via find_any
models_cancel(app, t, id: String) -> CmdResult<()>                                                            // 336
models_delete(app, t, id: String) -> CmdResult<()>                                                            // 349
```
`ModelRow` (`#[serde(rename_all="camelCase")]`): `id, name, maker, note, engine ("parakeet"|"whisper"|"llm"|"diarization"|"search"), size_mb: u64, speed: u8, accuracy: u8, languages: &str, state ("not_downloaded"|"downloading"|"downloaded"|"on_this_mac"), progress?: f32, partial?: f32 (only when not_downloaded), error?: String, cancelled: bool (omitted when false)`.
Events emitted (Rust -> frontend): `"models-changed"` (payload `()`; emitted l.282 download start, 315 finish/fail, 343 cancel, 362 delete, 119 after Windows move), `"model-download"` payload `DownloadProgress{ id: String, progress: f32 }` (throttled to every 250 ms or p>=1.0, l.295), `"model-download-failed"` payload `DownloadFailed{ id: String, message: String }` (l.328; **no frontend listener exists**: failure text reaches the UI through `models_list.error`/`llm_models_list.error`).
Post-download hooks (l.321-325): `is_llm` -> `commands::summarizer::requeue_waiting(&app)`; `Embedding` -> `commands::ask::reindex_soon(&app)`; speech -> `requeue_waiting_for_model(&app)` (meetings failed with code `no_model`).
`models_download` flow: `find_any(id)`; in-memory `downloads` map (re-entrant click is a no-op); `clear_cancelled`; VAD is auto-downloaded first for speech models when `store.locate(&VAD)==Missing` both with reuse false and true; `ModelStore::new(&data_dir,false)` (downloads always go to own dir); `report` closure writes progress into `downloads` map and emits `model-download`.

### 6b. `S/commands/summarizer.rs` (496 lines) - local-AI-relevant parts
`pub fn make_engine(cfg:&Config, data_dir:&Path) -> Result<Box<dyn SummaryEngine>, EngineError>` (l.50) -> `engine_for(cfg,data_dir,false)`; arm `_ =>` (engine "local") l.89-96; `fn local_ai` l.102; `pub fn gpu_fault(reason:&str)->bool` l.116; `fn summary_profile(cfg:&Config, gpu:&Gpu) -> eta::Profile` l.~121-134; `pub fn setup(app:&AppHandle, data_dir:PathBuf)` l.138 (wires `make_engine`, `summary_profile`); `pub fn requeue_waiting(app)` l.210; command `llm_models_list(t: State<TranscriberState>) -> CmdResult<Vec<LlmRow>>` l.410 (registered `lib.rs:260ish`); `local_server_models(address:String)->CmdResult<Vec<String>>` l.481 (Ollama/LM Studio; unrelated to engine, HTTP only). `LlmRow` (camelCase): `id, choice ("recommended"|"larger"), name, note, size_mb, state ("not_downloaded"|"downloading"|"downloaded"), progress?, partial?, error?, cancelled`. Settings keys read: `summaryEngine` (`"local"|"ollama"|"byok"`; anything else is local), `localModel`, `useGpu`, `ollamaAddress`, `ollamaModel`, `byokProvider`, `byokBaseUrl`, `byokModel`.

### 6c. `S/commands/settings.rs` `data_delete_all` (l.33-51)
`super::recorder::discard_for_wipe(&app)`; `crate::transcriber::relocate::stop(10 s)`; `storage.delete_all_data()` then `models = crate::transcriber::models::models_root(storage.root())` and `remove_dir_all(&models)` if it exists (error -> `tr_err("error.delete_model", e)`); `crate::keys::clear_all()`; `updates::remove_login_item`; `logging::delete_files()`; `app.exit(0)`. `data_info` l.68-76 adds `dir_size(models_root)` when it is outside the data dir (Windows). `gpu.json` lives in the data dir so `delete_all_data` removes it.

### 6d. `S/lib.rs` startup
- l.72 `logging::init(&logging::resolve_dir(std::env::var("TALAAN_DATA_DIR").ok(), dirs::home_dir(), dirs::data_local_dir()))`; l.73 `let root = commands::resolve_data_dir(std::env::var("TALAAN_DATA_DIR").ok(), dirs::data_dir())`; l.76 asset-protocol scope extension when `TALAAN_DATA_DIR` set.
- l.80-100 `#[cfg(windows)] let models_to_copy = if TALAAN_DATA_DIR unset/blank { dirs::data_local_dir().map(|local| { new = local_models_root(&local, &app.config().identifier); set_models_root(new.clone()); old = root.join("models"); left = relocate::prepare(&old,&new)...; (old,new,left) }) } else { None }` (MUST run before any worker; `set_models_root` is a `OnceLock`).
- Order after `Storage::new`: `app.manage(AppState)`; `log_startup` thread; `rec_cmd::setup` ; `commands::summarizer::setup(handle, root.clone())` (l.121); `commands::ask::setup(handle, root.clone())` (l.122); `commands::updates::setup`; `commands::transcriber::setup(handle, root)` (l.126); ...; l.136-146 background `copy_rest` thread (cfg windows).
- Command registrations related: `lib.rs:234-239` (`meetings_retry, meetings_retranscribe, models_list, models_download, models_cancel, models_delete`), `:247-248` (`gpu_status, gpu_retry`), `:260` (`summarizer::llm_models_list`), `:264` (`summarizer::local_server_models`).
- l.269-290 `RunEvent::ExitRequested` -> `log::info!("quitting ...")`; `exit_watchdog()` (l.40: hard `process::exit(0)` after `EXIT_GRACE`) unless `code == Some(tauri::RESTART_EXIT_CODE)`; `rec_cmd::stop_on_exit(app)`. NOTHING stops helper processes: `LocalAi::drop` and `RemoteWhisper::drop` kill them when their owners drop, otherwise the helper's own IDLE_EXIT (120 s, `llm/src/main.rs:26`) or stdin-EOF ends it.
- Updater: `S/commands/updates.rs:136-179` `install(app)` downloads, `update.install(bytes)` (l.177), `app.restart()` (l.179); no helper-closing code on the Rust side. Windows NSIS closes helpers (below).

### 6e. `S/logging.rs` (968 lines, 21 tests)
Engine-relevant: `OWN` (l.141) crate filter, `wanted()` (l.144), `short_target()` strips `talaan_lib::`/`talaan::`; `logging::text(&str)` redaction; `resolve_dir` (l.36); `for_log()` on StorageError. `log_scan.rs` (above). Log targets used by engine code today: `log::info!/warn!` calls in worker.rs, download.rs callers, helper.rs (`log::warn!("{} (out of memory: {})", logging::text(&detail), ..)` l.113), gpu.rs `record()`, local.rs `start()` ("local AI helper started on ..."), remote.rs load, commands/transcriber.rs.

---------------------------------------------------------------------------------------------------
## 7. Sub-crates in `src-tauri/`

### `talaan-llm-protocol` (`llm-protocol/`, v1.0.0, edition 2024, not published)
Deps: `serde 1 (derive)`, `serde_json 1`, `base64 0.22`. Files: `src/lib.rs` (195), `src/gpu.rs` (108), `src/stt.rs` (124).
- lib.rs: `pub mod gpu; pub mod stt; pub use gpu::{Device, DeviceKind};`
  `#[serde(tag="op", rename_all="snake_case")] enum Request { Ping, Complete(Completion), Embed(Embedding), Devices }`;
  `struct Embedding { model_path: String, texts: Vec<String>, #[serde(default="yes")] gpu: bool }`;
  `struct Completion { model_path: String, n_ctx: u32, system: String, user: String, #[serde(default, skip_serializing_if=None)] json_schema: Option<String>, max_tokens: u32, temperature: f32, #[serde(default="yes")] gpu: bool }`;
  `struct Backend { #[serde(default)] backend: String /*"vulkan"|"metal"|"cpu"*/, device: Option<String>, #[serde(default)] vram_mb: u64 }` with `Backend::cpu()`, `on_gpu()`;
  `#[serde(tag="event", rename_all="snake_case")] enum Event { Ready{#[serde(flatten)] backend: Backend}, Progress{phase: Phase, done: u32, total: u32}, Done(Reply), Embedded{vectors: Vec<Vec<f32>>}, Devices{devices: Vec<Device>, #[serde(default,skip_serializing_if=None)] chosen: Option<usize>}, Error{message: String} }`;
  `enum Phase { Loading, Reading, Writing }` (snake_case); `struct Reply { text: String, prompt_tokens: u32, output_tokens: u32, seconds: f32, truncated: bool }`; `pub fn encode<T: Serialize>(&T)->String`; `pub const FAIL_ENV: &str = "TALAAN_GPU"`. JSON lines: e.g. `{"op":"ping"}`, `{"event":"ready","backend":"vulkan","device":"NVIDIA GeForce RTX 5070 Ti","vram_mb":16303}`. Tests (4) pin the exact wire format incl. old-app compatibility (missing `gpu` = true; bare `{"event":"ready"}` = default Backend).
- gpu.rs: `enum DeviceKind {Cpu,Gpu,Igpu,Accel,Unknown}` (snake_case); `struct Device { index: usize, name: String, description: String, backend: String, kind: DeviceKind, memory_mb: u64 }`; `pub fn pick(&[Device]) -> Option<&Device>` (discrete GPU with most memory; never iGPU; first wins ties); `pub fn whisper_gpu_index(devices:&[Device], chosen:&Device) -> Option<i32>`; `pub fn bytes_to_mb(usize)->u64`. 5 tests.
- stt.rs: `SttRequest {Ping, Devices, Load{model_path:String, #[serde(default="super::yes")] gpu:bool}, Transcribe{samples:String /*base64 LE f32 16kHz*/, #[serde(default,skip_serializing_if=None)] language:Option<String>}}`; `SttPiece{start_ms:u64,end_ms:u64,text:String}`; `SttEvent {Ready{backend}, Devices{devices, chosen}, Loaded{backend}, Transcribed{pieces:Vec<SttPiece>, language:Option<String>}, Error{message}}` (Error variant is below line 60); `pub fn encode_samples(&[f32])->String`; `pub fn decode_samples(&str)->Result<Vec<f32>,String>`. 2 tests.
- Consumers: Talaan app (`S/commands/gpu.rs:10`, `S/summarizer/local.rs:16`, `S/summarizer/engines.rs:12`, `S/transcriber/remote.rs:7-8`), `llm/src/main.rs:23-24`, `stt/src/main.rs:10-12`, `stt/Cargo.toml`.

### `talaan-llm` (`llm/`, v1.0.0) binary only (`src/main.rs`, 340 lines, 2 tests)
Deps: `talaan-llm-protocol`, `serde_json 1`, `llama-cpp-2 =0.1.159` (default-features=false, features ["common"]); `[target.'cfg(macos)'.dependencies] llama-cpp-2 =0.1.159 features ["metal","common"]`; `[features] vulkan = ["llama-cpp-2/vulkan"]`. Loop: reads JSON lines from stdin, `IDLE_EXIT = 120 s`, `BATCH = 512`, `EMBED_MAX_TOKENS = 1024`, `GPU_MARGIN = 1 GiB`; fns `devices()`, `choose()` (uses `pick`), `backend()`, `fail_on_purpose()` (`TALAAN_GPU=fail` env), `model_params(Target)`, `load` (keeps one model loaded keyed by path/ctx), `threads()`, `context_params(n_ctx,on_gpu)`, `embed()` (LlamaPoolingType, L2-normalize), `complete()` (chat template via `LlamaChatMessage`, grammar from `json_schema`, `LlamaSampler`, temperature, progress events). Built as helper `talaan-llm` (CPU/Metal) and `talaan-llm-vulkan` (Windows, `--features vulkan`). Not a dependency of the app crate (the app spawns it by path).

### `talaan-stt` (`stt/`, v1.0.0) lib `talaan_stt` + bin `talaan-stt` (`src/lib.rs` 139, `src/main.rs` 91, 4 tests)
Deps: `talaan-llm-protocol`, `serde_json 1`, `whisper-rs 0.16.0 features ["raw-api"]`; dev `tempfile`; `[features] vulkan = ["whisper-rs/vulkan"]`. The app depends on it as `talaan-stt = { path = "stt" }` (never with vulkan). lib pub API: `is_noise(&str)->bool`; `threads()->usize`; `thread_count(logical:usize, windows:bool)->usize` (Windows 3/4 clamped, Mac all, clamped); `transcribe(state:&mut whisper_rs::WhisperState, samples:&[f32], language:Option<&str>, n_threads:usize) -> Result<(Vec<SttPiece>, Option<String>), String>`; `context_params(gpu_device:Option<i32>, flash_attn:bool) -> WhisperContextParameters<'static>`; `devices()->Vec<Device>` (ggml via whisper_rs_sys). bin `talaan-stt-vulkan.exe`: Ping/Devices/Load/Transcribe over stdin/stdout.
- App uses `talaan_stt::{is_noise, threads, context_params, transcribe}` only in `S/transcriber/engine.rs:44,40,161-164,173`. (`rg talaan_stt` outside: `logging.rs:13,141,819-821` strings.)

### `src-tauri/Cargo.toml` full dependency list (workspace root, package `talaan`)
`[build-dependencies]` tauri-build 2. `[dependencies]`: tauri 2 [tray-icon, image-png, macos-private-api, protocol-asset]; tauri-plugin-opener 2; tauri-plugin-dialog 2.7.0; serde 1 [derive]; serde_json 1; chrono 0.4.45 [serde]; dirs 7.0.0; rand 0.10.3; thiserror 2.0.21; cpal 0.18.2; rtrb 0.4.0; rubato 5.0.1; hound 3.5.1; **whisper-rs 0.16.0**; **parakeet-rs 0.3.8 [sortformer]**; **ort =2.0.0-rc.13 (default-features=false)** (comment: parakeet-rs 0.3.8 doesn't build against rc.14); reqwest 0.12 (default-features=false [rustls-tls, blocking, json]); tokio 1 [rt, time, macros]; sha2 0.10.9; strsim 0.11.1; whatlang 0.18.0; symphonia 0.5.5 [aac, isomp4, mp3, aiff, alac, caf]; **talaan-llm-protocol {path llm-protocol}**; **talaan-stt {path stt}**; keyring 3.6.3; rrule 0.14.0; chrono-tz 0.10.4; log 0.4.34.
`[target.'cfg(macos)'.dependencies]`: keyring 3.6.3 [apple-native]; **whisper-rs 0.16.0 [metal]**; block2 0.6.2; mach2 0.7.0; objc2 0.6.5; objc2-app-kit/av-foundation/core-audio/core-audio-types/core-foundation/foundation/user-notifications/event-kit 0.3.2.
`[target.'cfg(windows)'.dependencies]`: keyring 3.6.3 [windows-native]; **whisper-rs 0.16.0 (CPU only; vulkan lives in the helper)**; wasapi 0.26.0; winreg 0.56.0; windows 0.62 [many Win32 features]; windows-core 0.62; tauri-winrt-notification 0.8; tauri-plugin-single-instance 2. `[target.'cfg(not(android|ios))']`: tauri-plugin-autostart 2; tauri-plugin-updater 2. `[dev-dependencies]` tempfile 3.27.0; tiny_http 0.12.0.
`[features] updater-e2e = []`. `[profile.release]` codegen-units 1, lto true, opt-level 3, strip true. `[profile.dev.package."*"] opt-level = 3` (deps always optimized; whisper.cpp via CMake follows opt-level).
Runtime-only crates the engine swap would drop from the app: `whisper-rs` (3 cfg tables), `parakeet-rs`, `ort`, `talaan-stt`, `talaan-llm-protocol` (if protocol moves), maybe `whatlang`/`strsim` stay (pipeline/echo are Talaan logic). `ort` DirectML on Windows: `scripts/release-windows.ps1:23-32` builds `-p ort-sys`, copies `src-tauri/target/release/DirectML.dll` to `src-tauri/binaries/DirectML.dll` and bundles it as resource (no Cargo feature mentions DirectML; it comes from ort-sys/parakeet-rs defaults).

### `.cargo/config.toml`
- Root `.cargo/config.toml` (TRACKED in git; `git ls-files` lists it). Contents:
```
[env]
CMAKE_C_FLAGS_RELEASE = "-O2 -DNDEBUG"
CMAKE_CXX_FLAGS_RELEASE = "-O2 -DNDEBUG"
```
(with a comment: cmake-rs strips /O flags under the VS generator, so whisper.cpp and llama.cpp built at /Od in release on Windows ~10x slower). Cargo finds it because `src-tauri/` is under the root. `release-windows.ps1` (`Test-Optimized`) fails if the CMake caches lack `-O2`. `src-tauri/.cargo/config.toml` does NOT exist. `src-tauri/.gitignore` line 8 ignores `/binaries/`; root `.gitignore` ignores `src-tauri/target`, `src-tauri/gen/schemas`. A path/git dependency on an engine crate built from within Talaan inherits this config only when cargo runs inside the Talaan tree; the engine repo needs its own.

---------------------------------------------------------------------------------------------------
## 8. Build / packaging / CI

### Tauri config
- `src-tauri/tauri.conf.json`: `identifier "com.craftconceptsdigital.talaan"`, `productName "Talaan"`, `version "1.1.1"`; `build.beforeDevCommand = "sh scripts/build-llm-helper.sh && npm run dev"`, `build.beforeBuildCommand = "sh scripts/build-llm-helper.sh && npm run build"`, `devUrl http://localhost:1420`, `frontendDist ../dist`; `bundle.externalBin = ["binaries/talaan-llm"]` (only `talaan-llm`); updater endpoint `https://github.com/mmnzns/Talaan/releases/latest/download/latest.json`. NOTE: `sh scripts/...` runs with cwd = repo root (Tauri's cwd for hooks).
- `src-tauri/tauri.windows.conf.json`: `beforeDevCommand = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-llm-helper.ps1 && npm run dev"`, `beforeBuildCommand` same with `&& npm run build`; `bundle.targets ["nsis"]`; `nsis.installMode "currentUser"`, `installerHooks "windows/installer-hooks.nsh"`, `webviewInstallMode embedBootstrapper`, `bundleVCRuntime true`. Vulkan helpers and DirectML.dll are added only by `scripts/release-windows.ps1` via a generated `bundle.resources` config (`$resources['binaries/talaan-llm-vulkan.exe'|'binaries/talaan-stt-vulkan.exe'|'binaries/DirectML.dll']`, l.32-39).
- `src-tauri/tauri.macos.conf.json`: targets ["app","dmg"], `minimumSystemVersion "14.4"`, entitlements `./Entitlements.plist`.
- Dev artifact on this Mac: `src-tauri/binaries/talaan-llm-aarch64-apple-darwin` (6.5 MB, gitignored). The Mac sidecar is copied by Tauri to `Talaan.app/Contents/MacOS/talaan-llm`; `helper_path()` resolves next to `current_exe()`.
### Scripts
- `scripts/build-llm-helper.sh` (mac/linux): `cd src-tauri; cargo build --release -p talaan-llm; cp target/release/talaan-llm binaries/talaan-llm-$TRIPLE; cp ... target/debug/talaan-llm`.
- `scripts/build-llm-helper.ps1` (Windows): dot-sources `scripts/cpu-baseline.ps1` (x86-64-v3 CPU baseline for ggml, `Reset-StaleGgml`, `Get-GgmlCaches`, `Use-NinjaForGpu`); builds `talaan-llm` release -> `binaries/talaan-llm-<triple>.exe` and `target/debug/talaan-llm.exe`; if `VULKAN_SDK` (or `TALAAN_GPU=1` required) builds `-p talaan-llm` and `-p talaan-stt` with `--features vulkan --target-dir ($env:TALAAN_GPU_TARGET_DIR or target\gpu)` and copies `binaries\talaan-llm-vulkan.exe`, `binaries\talaan-stt-vulkan.exe` + `target\debug\`.
- `scripts/release-windows.ps1` (checks tag==version; calls build-llm-helper.ps1; builds `-p ort-sys`, copies DirectML.dll; generates release conf; `npm run tauri build -- --config $confPath [test config]`; asserts CMake caches have `-O2`/CPU baseline; asserts `talaan.exe` and `talaan-llm.exe` do NOT import `vulkan-1.dll` and the two `*-vulkan.exe` DO (dumpbin)). `scripts/gpu-helper-smoke.ps1 -Dir src-tauri/binaries` starts both Vulkan helpers (expects backend "cpu" on GPU-less runner). Others: `release-local.sh`, `build-signed-debug.sh` (`npm run tauri build -- --debug --bundles app`, signed with "Parlato Code Signing"), `upload-mac-release.sh`, `latest-json.mjs(+test)`, `updater-e2e/`.
### GitHub workflows (`.github/workflows/`, plus composite actions `.github/actions/{mac-signing,vulkan-sdk}`)
- `ci.yml` (84 lines): job `macos` (macos-latest): `npm ci`, `npm test`, `npm run build`, `sh scripts/build-llm-helper.sh`, `cargo test --manifest-path src-tauri/Cargo.toml --workspace` (rust-cache workspaces `src-tauri`). Job `windows` (windows-latest, `LIBCLANG_PATH=C:\Program Files\LLVM\bin`): same with `powershell ... scripts/build-llm-helper.ps1` then `cargo test ... --workspace`. Job `windows-gpu` (only `workflow_dispatch`; `TALAAN_GPU=1`, `TALAAN_GPU_TARGET_DIR=C:\tg`; uses `./.github/actions/vulkan-sdk`; builds Vulkan helpers; `cargo test --release ... -p talaan-llm -p talaan-stt -p talaan-llm-protocol --features talaan-llm/vulkan,talaan-stt/vulkan --target-dir C:\tg`; `scripts/gpu-helper-smoke.ps1`; uploads artifact `talaan-gpu-helpers`). Observed run times (gh): last runs 6-12 min wall (e.g. 2026-10-23: 12 min, 8 min, 12 min; latest 2026-10-30 10.5 min, failure).
- `release.yml` (265 lines): trigger tag push or `workflow_dispatch(platform)`; jobs `windows-test` (dispatch only; runs `scripts/release-windows.ps1` with Vulkan SDK, `gpu-helper-smoke.ps1`, uploads installer + `talaan-gpu-helpers` artifact), `draft` (ubuntu; creates draft release; checks tag vs tauri.conf.json), `windows` (windows-latest; Vulkan SDK action, `npm ci`, `./scripts/release-windows.ps1`, uploads installer/sig, `latest-json.mjs`), `macos` (gated by `vars.MAC_RELEASE_ON_CI == 'true'`; tauri-action builds `aarch64-apple-darwin` and signs with `mac-signing`; the `beforeBuildCommand` runs `build-llm-helper.sh`), `macos-test` (dispatch; `npm run tauri build -- --target aarch64-apple-darwin`, artifact `talaan-mac-test`), `latest-json` (ubuntu). `updater-test.yml` builds like the release Windows build without the Vulkan SDK (helpers don't take part in an update) and macOS with `npm run tauri build -- --target aarch64-apple-darwin --bundles app --config ...`.
### NSIS `src-tauri/windows/installer-hooks.nsh`
`!macro TalaanCloseHelpers` (used by `NSIS_HOOK_PREINSTALL` and `NSIS_HOOK_PREUNINSTALL`): `!insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-llm.exe" "${PRODUCTNAME}"`, same for `talaan-llm-vulkan.exe`, `talaan-stt-vulkan.exe`. Uninstall with "Delete app data" ticked: `RmDir /r "$APPDATA\Talaan"`, `RmDir /r "$LOCALAPPDATA\${BUNDLEID}\models"` (both asserted by `models.rs` test), `TalaanDeleteKey` for claude/openai/gemini/groq/cerebras/mistral/openrouter/compatible/notion/calendar-ics (test-matched to `keys::ACCOUNTS`). Any helper rename/new helper exe must be added here or updates fail with locked exes.

---------------------------------------------------------------------------------------------------
## 9. Frontend (src/)

### Tauri command names and TS payloads
`src/data/tauri/invoke.ts` exports `call<T>(cmd, args)` and `onEvent<T>(name, cb)` (used by the stores). Model-related commands (arguments are camelCase for Tauri `invoke`):
| TS method (SettingsStore, `src/data/settings.ts`) | command | args | returns | impl |
|---|---|---|---|---|
| `get()` (internal) | `models_list` | none | `SpeechModel[]` (speech + `engine:"diarization"` + `engine:"search"` rows) | `src/data/tauri/settingsStore.ts:43` |
| `get()` (internal) | `llm_models_list` | none | `LocalModel[]` | `settingsStore.ts:56` |
| `downloadModel(id)` | `models_download` | `{ id }` | `void` | `settingsStore.ts:91` |
| `cancelModelDownload(id)` | `models_cancel` | `{ id }` | `void` | `:95` |
| `deleteModel(id)` | `models_delete` | `{ id }` | `void` | `:99` |
| `localServerModels(address)` | `local_server_models` | `{ address }` | `string[]` | `:121` (Ollama only) |
| `gpuStatus()` / `retryGpu()` | `gpu_status` / `gpu_retry` | none | `GpuStatus` | `:201`, `:205` |
| `MeetingStore.retranscribe(id, model)` | `meetings_retranscribe` | `{ id, model }` | `void` | `src/data/tauri/meetingStore.ts:49` (interface `src/data/store.ts:47`) |
| (retry) | `meetings_retry` | `{ id }` | void | `meetingStore.ts` |
Events consumed in `settingsStore.ts` constructor: `"settings-changed"` (clears cache), `"models-changed"` (clears `models/speaker/search/localModels` caches, emits), `"model-download"` payload `{ id: string; progress: number }` (patches `state:"downloading", progress` on `models`, `speaker`, `search`, `localModels` by id). `"model-download-failed"` is NOT consumed anywhere in `src/`.
TS types (`src/data/settings.ts`): `SpeechModel { id; name; maker; note; engine: "whisper"|"parakeet"|"diarization"|"search"; sizeMb:number; speed:1|2|3; accuracy:1|2|3; languages:string; state:"not_downloaded"|"downloading"|"downloaded"|"on_this_mac"; progress?:number; partial?:number; error?:string; cancelled?:boolean }` (l.67-83). `LocalModel { id; choice:"recommended"|"larger"; name; note; sizeMb; state:"not_downloaded"|"downloading"|"downloaded"; progress?; partial?; error?; cancelled? }` (l.86-98). `GpuStatus { state:"metal"|"in_use"|"off"|"unavailable"|"failed"; reason?:"no_vulkan"|"no_helper"|"no_gpu"|"no_start"; device?:string; vramMb?:number; detail?:string }` (l.42-51). SettingsStore interface lines 185-189 (`downloadModel`, `cancelModelDownload`, `deleteModel`), 197 `localServerModels`, 235 `gpuStatus`, 237 `retryGpu`. Any engine change that renames `engine` values (`"parakeet"` groups Parakeet/Unified/Nemotron) or `state` strings breaks these files.
### Settings keys (settings.json, camelCase; `Settings` in `src/data/settings.ts:~100-175`)
`activeSpeechModel: string` (l.127), `speechModels: SpeechModel[]` (not stored), `speakerModel: SpeechModel` (not stored), `separateSpeakers: boolean`, `searchModel: SpeechModel` (not stored), `reuseModels: boolean`, `meetingLanguage: "auto"|string`, `useGpu: boolean`, `localModel: "recommended"|"larger"` (l.149), `localModels: LocalModel[]` (not stored), `summaryEngine`, `summarizeAutomatically`, plus others. `settingsStore.ts:83` strips `speechModels, speakerModel, searchModel, localModels` before `settings_set`. `get()` coerces an unknown `activeSpeechModel` to `defaultSettings.activeSpeechModel` ("whisper-turbo").
### `src/data/defaultSettings.ts`
Hand-copied catalog (must match Rust ids in the same order; Rust test `models.rs:459` enforces order of `id: "..."`): `speechModels` ids in order: whisper-turbo, parakeet-v3, whisper-small, whisper-large-v3, whisper-large-v2, whisper-medium, whisper-base, whisper-tiny, parakeet-ultra, parakeet-v2, parakeet-unified-en, nemotron-3.5 (l.~60-71); `activeSpeechModel: "whisper-turbo"` (l.72); `speakerModel` id "diarization" (l.73); `separateSpeakers: true` (74); `searchModel` id "search" (75); `reuseModels: true` (76); `useGpu: true` (79); `localModel: "recommended"` (85); `localModels` ids "qwen3.5-4b" (choice recommended, sizeMb 2741) and "qwen3.5-9b" (larger, 5681) (l.87-90; comment "Mirrors the Rust catalog (src-tauri/src/summarizer/models.rs)"). `withDefaults(stored)` merges.
### Files using model/GPU commands or settings (file:line of the key uses)
- `src/screens/Settings/ModelsTab.tsx` (speech models UI): 28 `showMore ?? more.some(m => m.id === s.activeSpeechModel)`, 30 `t("settings.reuse_found",{count,size})` (counts `state === "on_this_mac"`), 58-59 `separateSpeakers` Switch, 62 `searchModel`, 70 `reuseModels` Switch, 96-112 `activeSpeechModel` selection (`toast(t("settings.model_download_first",{name}))` if not installed), 99 progress estimate `Math.ceil(((1 - progress) * sizeMb)/250)` minutes, 131-132,139,145 `downloadModel/cancelModelDownload/deleteModel`, 163-180 (same for speaker/search rows).
- `src/screens/Settings/SummaryAITab.tsx`: 166 `s.localModels.find(m => m.choice === s.localModel) ?? s.localModels[0]`, 171-174 `<Select value={s.localModel}>` option text `t("settings.local_option",{choice,name,size})`, 184 `deleteModel(model.id)`, 189 `downloadModel(model.id)` (`t("settings.download_size")`), 196 `cancelModelDownload(model.id)`, 217 `localServerModels(s.ollamaAddress)`.
- `src/screens/Settings/StatusTab.tsx`: 20 `<SetupCheck gpuKey={s.useGpu}>`, 23 `Switch checked={s.useGpu} onChange={v => update({ useGpu: v })}`. `src/screens/Settings/SetupCheck.tsx:35-38,83` calls `probeSetup(store, recorderClient, testSystemAudio, retryGpu)`.
- `src/screens/Onboarding/SetupDownload.tsx`: 9 `allModels = [...s.speechModels, ...s.localModels]`; 28 `Promise.all(todo.map(i => store.downloadModel(i.id)))`; 34 `store.update({ activeSpeechModel: speech.id })`; 39 cancel all `state==="downloading"` items; 47 `resumable` (partial>0); 123,143 `store.downloadModel(m.id)`; 196-197 progress weighted by `sizeMb * partial`.
- `src/screens/Onboarding/OnboardingScreen.tsx`: 180-181 "needs download" checks (`state === "not_downloaded"`), 392-395 progress % from `progress`/`partial`, 478-499 speech picker (tiers) with `activeSpeechModel`, 510-572 `DownloadRow` (props item {name,state,progress?,sizeMb,error?}), 590 local model row `s.localModels.find(m => m.choice === (s.localModel ?? "recommended"))`, 599-600 download/cancel, 720-721 `chosen speech`/`local`.
- `src/screens/Onboarding/TestRecording.tsx:100` `settings.speechModels.find(m => m.id === settings.activeSpeechModel)`.
- `src/screens/Meeting/MeetingHeader.tsx:333` `store.retranscribe(meeting.id, model)`; sheet UI 340-366 (`retranscribe*` keys); `StatusBanner.tsx:32` `meeting.status === "failed" && meeting.error?.code === "no_model"` (the string `no_model` is `worker::NO_MODEL`).
- `src/lib/setupDownload.ts`: 4 comment (downloads run in Rust), 16 `sizeMb`, 32,37 resumable/stopped lists (`state==="not_downloaded" && partial>0`, `cancelled`, `error`), 43 `localModel(s)`, 51-60 active speech/local, 66-77 ETA math (bytes = sizeMb*1e6, speed measurement). `src/lib/setupCheck.ts`: 9 `FixAction` includes `"retryGpu"`, 84 gpu row with `t("settings.setup_gpu_retry")`, 143 active speech check, 155 `!s.separateSpeakers`, 177 local model check, 214 `regraphics(p, store: Pick<SettingsStore,"get"|"gpuStatus">)`, 243 `probeSetup(..., retryGpu)`, 250 `store.localServerModels`, 258 `(retryGpu ? store.retryGpu() : store.gpuStatus())`. `src/lib/modelTiers.ts`: `TIERS` ids whisper-turbo / parakeet-v3 / whisper-small (labels `settings.tier_best|tier_fast|tier_old`), `splitByTier`, `modelNote(lang, m, windows)` uses i18n key `settings.model_note_${m.id}` (+ `_windows` for parakeet-v3).
- Mock/browser mode: `src/data/mock/settings.ts` (82-91 patches `speechModels/localModels/speakerModel/searchModel` state; 89 `downloadModel` simulates progress by `0.1*min(1, 1000/sizeMb)`; 116 cancel; 236 `deleteModel`; 258 `localServerModels`; 264-272 `gpuStatus`/`retryGpu`), `src/screens/Stage/ScreensStage.tsx:86` (`localModels: settings.localModels.map(none)`).
### i18n keys involved (`src/lib/i18n/messages/{settings,onboarding,meeting,common}.ts`, EN/FR/ES; Rust keys in `S/i18n.rs`)
- settings.ts: `settings.cancel`, `.delete`, `.download`, `.download_size`, `.model_section`, `.model_section_desc`, `.model_use`, `.model_download_first`, `.model_progress`, `.model_download_mb`, `.model_tech`, `.search_title`, `.search_desc`, `.reuse_title`, `.reuse_found_one/_other`, `.reuse_none`, `.more_models_show/_hide/_desc`, `.tier_best/_fast/_old`, `.speakers_title/_desc` (warn on speakers, unrelated), `model_note_<id>` for every id (`parakeet-v3_windows`, whisper-small, whisper-large-v3, whisper-large-v2, whisper-medium, whisper-base, whisper-tiny, parakeet-ultra, parakeet-v2, parakeet-unified-en, nemotron-3.5, diarization, search, qwen3.5-4b, qwen3.5-9b), `settings.local_model_title/_desc`, `.local_recommended`, `.local_larger`, `.local_option`, `.engine_local*`, `.setup_speech*`, `.setup_speakers_off/ok/missing`, `.setup_summary_downloading/nolocal/...`, `.setup_gpu`, `.setup_gpu_metal/off/in_use/in_use_plain/no_gpu/no_vulkan/no_helper/no_start/failed/retry`. Line ranges (EN block): 157-187, 192-238, 360-389.
- onboarding.ts: `onboarding.modelOnThisComputer`, `modelDownloaded`, `modelDownloading`, `modelSize`, `modelTitle`, `modelDesc`, `modelGroup`, `localTitle`, `localDesc`, `localDescNoSize`, `localDownloading`, `downloadName`, `setupTitle`, `setupDesc`, `setupButtonNoTime`, `setupKeepUsing`, `setupDownloading`, `setupReady`, `setupSpeech`, `setupSummary`, `setupItemReady`, `setupButtonRest`, `setupStopped`, `setupRetry`, `setupDescSpeech`, `setupDescSpeechServer`.
- meeting.ts `meeting.retranscribe*` (Menu, Title, Body, Model, Used, NoModel, Edits, Replaced, Names, SummaryWaits, SummaryAgain).
- Rust i18n (`S/i18n.rs`): `error.cancelled`(152), `error.download_speech_model`(175), `error.delete_model`(180), `ai.download_local`(228), `ai.needs_ai`, `local_ai.no_start/too_slow/crashed/out_of_memory`(264-273), `ask.stopped/needs_ai/unreadable`, `download.network/corrupt/save_failed/stalled/not_a_model`(379-419), `notify.transcript_ready`, `notify.summary_ready`.

---------------------------------------------------------------------------------------------------
## 10. Tests

### Commands
```
npm test                                           # Vitest (vitest run); 38 files under src/ + vitest picks only src tests
cargo test --manifest-path src-tauri/Cargo.toml                 # app crate only
cargo test --manifest-path src-tauri/Cargo.toml --workspace     # what CI runs (app + talaan-llm + talaan-llm-protocol + talaan-stt)
sh scripts/build-llm-helper.sh                     # CI builds the helper BEFORE cargo test (tests that spawn it expect target/debug/talaan-llm)
node --test scripts/latest-json.test.mjs           # (not in CI npm test; plain node test)
# ignored/live, need models or GPU:
TALAAN_SMOKE_MEETING=<dir> cargo test --release --manifest-path src-tauri/Cargo.toml pipeline_smoke|engine_smoke|vad_smoke|diarize_smoke -- --ignored --nocapture
TALAAN_MODELS="parakeet-v3 vad" cargo test --release real_download -- --ignored
TALAAN_LLM_MODEL=<gguf> cargo test --release local_ai_smoke -- --ignored ;  TALAAN_LLM_DIR=<folder> cargo test --release bakeoff -- --ignored
TALAAN_DATA_DIR=<scratch> cargo test --release gpu_whisper_live -- --ignored --nocapture --test-threads=1
cargo build -p talaan-llm && cargo test ask_smoke -- --ignored --nocapture ; cargo test embed_smoke|gpu_here -- --ignored --nocapture
```
Timing: not run (read-only). CI evidence: whole CI workflow (npm ci + vitest + vite build + helper build + cargo test --workspace, cold-ish cache) 8-12 min per OS on GitHub runners; local incremental runs are much faster since `[profile.dev.package."*"] opt-level = 3` makes first dependency builds (whisper.cpp/llama.cpp/ort) slow. The unit tests themselves are quick; the worker/download/relocate tests use temp dirs and a local `tiny_http` server.
### Rust tests (counts per file; `#[test]`, `ign` = `#[ignore]`)
transcriber/: models.rs 7 (catalog_is_consistent, the_preview_lists_the_same_speech_models_in_the_same_order, own_copy_wins_and_partial_files_do_not_count, another_apps_copy_is_found_only_when_reuse_is_on, the_old_folder_is_read_until_the_move_is_done, without_a_separate_models_folder_there_is_no_old_one, windows_models_live_in_the_bundle_id_folder); download.rs 14 (+1 ign real_download; local tiny_http server: resume, Range ignored, damaged file, stall, HTML page, cancel marker, partial_fraction); relocate.rs 11 (SERIAL mutex, `reset_for_tests`); remote.rs 5; fallback.rs 7 (+1 ign gpu_whisper_live); engine.rs 3 (all ign smoke); vad.rs 7 (+1 ign vad_smoke); diarize.rs 5 (+1 ign); pipeline.rs 7 (+1 ign pipeline_smoke); echo.rs 8; merge.rs 4; worker.rs 14 (fake `Engine` impls `Echoes`, `FallsBack`, `WholeTrack` detector, `sparse()` files of exact model size, `reuse_roots: Some(vec![])`).
summarizer/: local.rs 12 (+2 ign; fake shell-script helpers from `helper::fake`), models.rs 1, engines.rs 11, summarize.rs 12 (+1 ign), worker.rs 15 (settings `{"summaryEngine":"local","localModel":"recommended"}`), followup 7, chunk 4, parse 10, prompt 15, templates 3.
Others touching these modules: helper.rs 5, gpu.rs 6 (+1 ign), commands/gpu.rs 4, commands/summarizer.rs 1 (`only_a_real_gpu_fault_counts_against_the_gpu_after_a_follow_up`), commands/ask.rs 4 (+1 ign `ask_smoke`), commands/mod.rs (`no_synchronous_command_builds_a_window` scans transcriber.rs/summarizer.rs/settings.rs/ask.rs sources), eta.rs 6, logging.rs 21 (OWN filter incl. talaan_stt), log_scan.rs (scans src, stt/src, llm-protocol/src), ask/{answer 3, index 3, mod 1 ign, passages 5, search 4}, keys.rs/deeplink.rs (nsh), llm-protocol 11 (4+5+2), stt 4, llm 2.
### Vitest files that touch these (all under `src/`)
`data/tauri/tauriStores.test.ts` (mocks `invoke`; `models_list`, `llm_models_list` fixtures at l.17-25; `model-download` event l.193 -> state downloading; `models_download` call l.198; `models-changed` -> second `models_list` call l.200-202; localModels[0].id "qwen3.5-4b" l.148; stale `activeSpeechModel:"parakeet"` -> "whisper-turbo" l.179-180), `data/mock/settings.test.ts`, `data/defaultSettings.test.ts`, `lib/setupDownload.test.ts` (uses defaultSettings.speechModels/localModels, `parakeet-v3` on_this_mac), `lib/setupCheck.test.ts` (localModels, `useGpu`, `gpuStatus` fake store), `lib/modelTiers.test.ts` (`TIERS[0].id === defaultSettings.activeSpeechModel`), `screens/Settings/StatusTab.test.ts`, `lib/eta.test.ts`, i18n tests (`lib/i18n/i18n.test.ts`, `sectionsMissing.test.ts`, `tabNames.test.ts`: every key present in EN/FR/ES), `lib/diagnostics.test.ts`, `data/errors.test.ts`.

---------------------------------------------------------------------------------------------------
## 11. Model ids stored in settings, and where models live on THIS Mac (listing only)

Stored in `settings.json` (data dir root; keys, not stored for catalog rows): `"activeSpeechModel": "<speech id>"` (one of whisper-turbo, parakeet-v3, whisper-small, whisper-large-v3, whisper-large-v2, whisper-medium, whisper-base, whisper-tiny, parakeet-ultra, parakeet-v2, parakeet-unified-en, nemotron-3.5; default "whisper-turbo"; unknown/`"vad"` -> default), `"localModel": "recommended"|"larger"` (maps to qwen3.5-4b / qwen3.5-9b; LLM is NOT stored by id), `"reuseModels": bool`, `"useGpu": bool`, `"separateSpeakers": bool`, `"summaryEngine": "local"|"ollama"|"byok"`. On this Mac's settings.json: `activeSpeechModel "whisper-turbo"`, `localModel "recommended"`, `reuseModels true`, `separateSpeakers true`, `summaryEngine "local"`, `useGpu true`. Non-catalog ids in persisted data: per-meeting `TranscriptionInfo.model: String` (S/storage/model.rs:250-251, the model id at transcription time) and `retranscribe_model: Option<String>` (model.rs:207; set by `Storage::start_retranscribe(id, model)` edits.rs:77-94). `speed.json` (data dir) keys like `"stt:whisper-turbo:gpu"`, `"summary:byok:openai:remote"` come from `eta::Profile.key` (`"stt:{model.id}:{cpu|gpu}"`), so changing model ids or device tags invalidates learned rates. Diagnostics report allowlist of settings keys at `S/commands/diagnostics.rs:21-23` (includes activeSpeechModel, separateSpeakers, reuseModels, useGpu, localModel).
Disk today (`~/Library/Application Support/Talaan/`): `meetings/`, `models/`, `settings.json`, `speed.json` (and `~/Library/Logs/Talaan/talaan.log`). `Talaan/models/` contains: `diarization/` (nemotron3_diar_v3.onnx 400_506_656), `parakeet-ultra/` (encoder-model.int8.onnx 649_524_002, decoder_joint-model.int8.onnx 18_203_490, nemo128.onnx, vocab.txt, config.json), `search/` (Qwen3-Embedding-0.6B-Q8_0.gguf 639_150_592), `vad/` (ggml-silero-v6.2.0.bin 885_098), `whisper-turbo/` (ggml-large-v3-turbo-q5_0.bin 574_041_195). NO LLM model is downloaded on this machine; by code an LLM would be `~/Library/Application Support/Talaan/models/qwen3.5-4b/Qwen3.5-4B-Q4_K_M.gguf` (or `qwen3.5-9b/Qwen3.5-9B-Q4_K_M.gguf`); embeddings are in the same folder family (`models/search/`). So on Mac ALL model kinds share `<data>/models/<id>/<file>`; on Windows `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\models\<id>\<file>` (old: `%APPDATA%\Talaan\models`). Partials: `<file>.part`, `.cancelled` marker, `.talaan-move-<id>` temp during Windows relocate. Reuse-from-other-app source (Parlato) at `~/Library/Application Support/com.craftconceptsdigital.parlato/` (has `Models/`, `ParakeetModels/`; both exist but the checked listing shows `Models/imported` only and `ParakeetModels/` empty). A third app folder `com.craftconceptsdigital.ensayo` exists but is not used by Talaan.

---------------------------------------------------------------------------------------------------
## 12. Risk checklist for the plan (things easy to miss)
1. `models.rs:459` test needs `src/data/defaultSettings.ts` id order == CATALOG order; if CATALOG moves to the engine crate the test must move or be replaced (or the TS list generated).
2. Move of `windows_models_live_in_the_bundle_id_folder` test is coupled to `tauri.conf.json` + `installer-hooks.nsh` (stays in the app).
3. `logging::OWN` and `log_scan::rust_files()` must include the new crate; `logging::text` redaction is app-only so engine errors must be redacted by the app at the boundary (the log scan forbids `{:?}` and path/name/url fields).
4. `EngineError`/local-AI error strings: `"cancelled"` literal, GPU-failure `contains("memory")`, English crash text, `local_ai.*` i18n selection from `Death.out_of_memory`; engine crate should expose typed errors, app keeps i18n.
5. `VAD` is `EngineKind::Whisper` and downloaded implicitly with every speech model (commands/transcriber.rs:299-304); `find()` returns VAD/DIARIZATION/SEARCH but `find_any` filters VAD back out.
6. `find()` vs `find_llm()` are separate; `models_delete/cancel/download` resolve through `find_any` (speech + diarization + search + LLM), while `models_list` (no LLM) + `llm_models_list` feed the UI.
7. LLM downloads use `reuse=false` (`Location::Own` only), speech `Config.reuse` honours `reuseModels` for locate but downloads always go to the own dir (`ModelStore::new(.., false)` at 281/287).
8. Helper binary packaging: only `binaries/talaan-llm` is in `externalBin`; Vulkan helpers added via release-script resources; Mac DMG relies on `build-llm-helper.sh` in `beforeBuildCommand`; `CI macos/windows jobs` run helper build before `cargo test` because some tests/smoke expect `target/debug/talaan-llm`. NSIS must close all helper exes by name.
9. `talaan-stt` is both a lib (linked into the app: `is_noise`, `threads`, `context_params`, `transcribe`) and the Vulkan helper bin; `whisper-rs 0.16.0` appears in three places in the app Cargo.toml (generic, macos+metal, windows) and `raw-api` in stt.
10. `ort =2.0.0-rc.13` pin and DirectML.dll bundling (release-windows.ps1) disappear/move with parakeet-rs/ort; `[profile.dev.package."*"] opt-level=3` and root `.cargo/config.toml` CMake `-O2` flags are what keep whisper/llama usable in dev/release on Windows.
11. `TranscriberState` (commands/transcriber.rs:21) owns `data_dir`, `downloads`, `failed` maps and is the dependency of `commands/gpu.rs`, `commands/summarizer.rs::llm_models_list`, `commands/meetings.rs`, `commands/import.rs`, `commands/recorder.rs`; progress/failed maps stay app-side unless the engine crate provides its own download manager.
