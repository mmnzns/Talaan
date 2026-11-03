# voice-engine v0.2.2 — exact API (read from `git show v0.2.2:<path>`, ~/Developer/voice-engine)

Repo: https://github.com/mmnzns/voice-engine, tag v0.2.2. Workspace edition 2024, resolver 3, `publish = false` (git deps only).
Needs a recent Rust (edition 2024; models uses `File::lock_shared`/`try_lock` => Rust >= 1.89).

## 1. CARGO

Package names / lib names (all version 0.2.2):
| package | lib name | notes |
|---|---|---|
| voice-engine-protocol | voice_engine_protocol | deps: serde, serde_json, base64 |
| voice-engine-models | voice_engine_models | deps: dirs, reqwest(rustls, blocking), sha2. No features |
| voice-engine-stt | voice_engine_stt | whisper-rs 0.16.0, parakeet-rs 0.3.8 (sortformer), ort =2.0.0-rc.13 |
| voice-engine-llm | voice_engine_llm | llama-cpp-2 =0.1.159 (metal on macOS) |
| voice-engine-client | voice_engine_client | deps: protocol, serde_json, reqwest |

Features:
- voice-engine-stt: `vulkan = ["whisper-rs/vulkan"]` (needs VULKAN_SDK to build, vulkan-1.dll to run; "never linked into an app's main program" = helper builds only);
  `directml = ["ort/directml", "dep:windows"]` (Windows only; Parakeet on discrete card in the APP's own process; DirectML.dll from System32, Win10 1903+). No default features.
- voice-engine-llm: `vulkan = ["llama-cpp-2/vulkan"]`. Mac Metal is automatic (target cfg).
- protocol, models, client: none.
- voice-engine-stt NEVER depends on llm (no duplicate ggml symbols; tests/no_llama.rs enforces).

README "Use it in an app" (verbatim):
```toml
voice-engine-protocol = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2" }
```
"Every engine crate in one app uses the same tag." So an app writes (derived, same pattern):
```toml
voice-engine-protocol = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2" }
voice-engine-models   = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2" }
voice-engine-client   = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2" }
voice-engine-stt      = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2", features = ["directml"] }  # in-process Parakeet/Whisper/VAD/diarize; directml optional, Windows
voice-engine-llm      = { git = "https://github.com/mmnzns/voice-engine", tag = "v0.2.2" }  # only in the helper bin
```
(Only protocol's line is literal in the README; the rest is the same form. Repo may be private: CI/cargo needs git auth — not documented in the repo.)

Dev override (README "Try an engine change in an app before tagging"; `src-tauri/.cargo/config.toml`, git-ignored):
```toml
[patch."https://github.com/mmnzns/voice-engine"]
voice-engine-protocol = { path = "C:/Dev/voice-engine/crates/protocol" }
voice-engine-llm      = { path = "C:/Dev/voice-engine/crates/llm" }
voice-engine-stt      = { path = "C:/Dev/voice-engine/crates/stt" }
voice-engine-models   = { path = "C:/Dev/voice-engine/crates/models" }
```

Helper main.rs. The repo's own bins use prefix "VOICE_ENGINE"; an app uses its own prefix (README: "Each helper program stays in the app as a 3-line main.rs"):
```rust
// llm helper (app's src-tauri/src/bin or its own tiny crate)
fn main() {
    voice_engine_llm::run(voice_engine_protocol::HelperConfig { env_prefix: "TALAAN" });
}
// stt helper
fn main() {
    voice_engine_stt::run_helper(voice_engine_protocol::HelperConfig { env_prefix: "TALAAN" });
}
```
Signatures: `pub fn voice_engine_llm::run(config: HelperConfig)`; `pub fn voice_engine_stt::run_helper(config: HelperConfig)` (both re-exported at crate root; return `()`).
Helper crate deps needed: llm helper -> voice-engine-llm + voice-engine-protocol (+ `features=["vulkan"]` for the GPU variant); stt helper -> voice-engine-stt + voice-engine-protocol (+vulkan).
BUT the recommended route is NOT to compile helpers in the app: use the reusable workflow (§6), which builds the engine's own bins (named voice-engine-llm / voice-engine-stt, with the app's chosen sidecar names) with the engine's .cargo/config.toml.
Env vars from HelperConfig: `<PREFIX>_GPU` (=`fail` forces GPU-build failure for fallback test), `<PREFIX>_LLM_IDLE_SECS` (tests only).

## 2. voice-engine-models  (crate path `voice_engine_models`)

Root re-exports (lib.rs):
```rust
pub mod catalog; pub mod download; pub mod imports; pub mod legacy; pub mod moving; pub mod shared;
pub use catalog::{GpuSupport, Kind, Licence, LlmInfo, ModelFile, ModelInfo};
pub use download::{DownloadError, Progress};
pub use legacy::{App, LegacyEntry, LegacySource};
pub use moving::{ImportedGguf, MoveHooks, MoveReport};
pub use shared::{ModelHandle, Released, SharedStore, Status, StoreError};
```
### App (legacy.rs)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum App { Talaan, Ensayo, Parlato }
impl App { pub const ALL: [App; 3] = [App::Talaan, App::Ensayo, App::Parlato];
           pub fn bundle_id(self) -> &'static str }  // "com.craftconceptsdigital.{talaan|ensayo|parlato}"
```
### SharedStore (shared.rs; move_in is in moving.rs)
```rust
#[derive(Debug)] pub struct SharedStore { /* app, root, old_folders, read_only, downloading */ }  // not Clone
impl SharedStore {
  pub fn open(app: App) -> std::io::Result<SharedStore>;                 // default_dir(), or $VOICE_ENGINE_MODELS_DIR (non-empty) == open_at. Only the real-folder variant enables move_in.
  pub fn open_at(app: App, dir: PathBuf) -> io::Result<SharedStore>;     // creates dir/.engine/{users,locks,moved}; NEVER moves anything in
  pub fn read_only(app: App, dir: PathBuf) -> SharedStore;               // infallible; creates nothing; writers -> PermissionDenied
  pub fn default_dir() -> Option<PathBuf>;     // dirs::data_local_dir()/"Craft Concepts Digital"/"Models"
  pub fn root(&self) -> &Path;
  pub fn model_dir(&self, id: &str) -> PathBuf;                         // root/<id>
  pub fn status(&self, id: &str) -> Status;                              // unknown id => Status::Missing
  pub fn list(&self) -> Vec<(&'static ModelInfo, Status)>;               // catalog order
  pub fn use_model(&self, id: &str) -> Result<(), StoreError>;           // writes users/<id>/<app> marker
  pub fn download(&self, id: &str, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<PathBuf, StoreError>; // blocking; writes marker first; returns model dir
  pub fn hold(&self, id: &str) -> Result<ModelHandle, StoreError>;       // shared lock; Err(NotInstalled) unless complete
  pub fn release(&self, id: &str) -> io::Result<Released>;
  pub fn release_all(&self) -> io::Result<()>;                           // "Delete all data"
  pub fn collect(&self) -> io::Result<()>;                               // erase unused models + .part >7d; call at start
  pub fn move_in(&self, used: &[&str]) -> io::Result<MoveReport>;        // see below
  // #[doc(hidden)] test hooks: for_move_test, status_with, download_with, move_in_with, move_in_with_hooks
}
```
Layout: `<root>/<id>/<files>`, `<file>.part`, `.cancelled`; `<root>/.engine/{users/<id>/<app>, locks/<id>.use, locks/<id>.download, moved/<app>}`. Only the engine writes `.engine`. Locks are OS file locks (vanish on crash). Never delete a model folder yourself.
Valid id: first char ASCII alnum, rest alnum/./-/_ (else io InvalidInput; Missing for status).
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
  Installed { dir: PathBuf, users: Vec<App>, used_by_me: bool },
  Downloading { fraction: f32, bytes: u64, by_me: bool },
  Partial { fraction: f32, bytes: u64, cancelled: bool },
  Missing,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Released { Erased, Kept { users: Vec<App> } /* empty = another app has it loaded; collect erases later */, NotUsing }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError { UnknownModel(String), NotInstalled(String), DownloadingElsewhere, Download(DownloadError), Io(String) }
// impl Display, std::error::Error, From<io::Error> (-> Io(kind string)), From<DownloadError>
#[derive(Debug)] pub struct ModelHandle { /* dir, lock */ }  impl ModelHandle { pub fn dir(&self) -> &Path }  // keep while model loaded; drop on unload
```
### download.rs
```rust
#[derive(Debug, Clone, Copy, PartialEq)] pub struct Progress { pub fraction: f32, pub bytes: u64, pub total: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError { Offline, Refused { status: u16 }, DiskFull, Checksum { file: String }, Stalled, Cancelled, Io(String) }
impl DownloadError { pub fn is_failure(&self) -> bool }   // false for Cancelled & Stalled (.part kept, next download resumes)
// Display + Error. pub const IDLE: Duration = 30s. pub fn download_files(files:&[ModelFile], dir:&Path, cancel:&AtomicBool, idle:Duration, progress:&mut dyn FnMut(Progress)) -> Result<(), DownloadError>
```
### moving.rs
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MoveReport {
  pub moved: Vec<String>,                       // adopted from this app's old folders
  pub linked: Vec<String>,                      // hard-linked from OTHER apps' folders
  pub imported: Vec<ImportedGguf>,              // Parlato only
  pub left_for_later: Vec<(String, String)>,    // (id, "in use" | "hash mismatch" | "link failed" | "move failed")
  pub already_done: bool,
}
impl MoveReport { pub fn skipped() -> MoveReport }  // already_done=true
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ImportedGguf { pub path: PathBuf, pub id: String }
// imports::import_id(path:&Path) -> io::Result<String>  // "import-" + 12 hex
```
move_in semantics: no-op + `already_done` on `open_at`/env-override store or once `.engine/moved/<app>` exists. Own old files are RENAMED into `<root>/<id>/` and a hard link left at old path (old app version still works; no extra disk). Other apps' complete models are hard-linked in (never moved), that app recorded as a user. Model counts only if every file has the catalog's exact size. File open/locked => left, "in use", marker not written, retried next start. `used` = ids the app's settings point to (recorded as used by this app). Call first thing, before opening a model/starting a helper. Order at start: open -> move_in -> collect -> list.

Talaan legacy folders `move_in` reads (LegacySource::all; `models/<catalog id>/<catalog file names>`, no renames):
- Mac: `~/Library/Application Support/Talaan/models/<id>/`   (= `<data_local_dir>/Talaan/models`)
- Windows: `%LOCALAPPDATA%\com.craftconceptsdigital.talaan\models\<id>\`  AND, if present, `%APPDATA%\Talaan\models\<id>\` (older Roaming)
- Talaan ids: whisper-tiny, whisper-base, whisper-small, whisper-medium, whisper-large-v2, whisper-large-v3, whisper-turbo, parakeet-v2, parakeet-v3, parakeet-ultra, parakeet-unified-en, nemotron-3.5, vad, diarization, search, qwen3.5-4b, qwen3.5-9b
- (Also scanned for other apps: Ensayo `<local>/com.craftconceptsdigital.ensayo/models/<id>` and `.../models/llm/<id>`; Parlato `<local>/com.craftconceptsdigital.parlato/{Models,ParakeetModels/*,LlmModels}`.)
Shared folder: Mac `~/Library/Application Support/Craft Concepts Digital/Models`; Windows `%LOCALAPPDATA%\Craft Concepts Digital\Models`.
`VOICE_ENGINE_MODELS_DIR`: if set non-empty, `SharedStore::open` == `open_at(app, that dir)` (no move_in, `already_done`). Tests/dev must use `open_at` with their own dir, never the real folder.
`LegacySource::on_this_computer(app) -> Vec<LegacySource>`; `LegacySource{app,root,entries:Vec<LegacyEntry>,imports_rel:Option<&'static str>}`; `LegacyEntry{id,rel,files:&[(&str,&str)],verify_hashes:bool}`.

### catalog.rs  (`voice_engine_models::catalog`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind { Whisper, Parakeet, ParakeetUnified, Nemotron, Vad, Diarization, Embedding, Llm }
impl Kind { pub fn is_speech(self) -> bool }   // Whisper|Parakeet|ParakeetUnified|Nemotron
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Licence { pub spdx: &'static str, pub url: &'static str }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct GpuSupport { pub mac_metal: bool, pub windows_vulkan: bool, pub windows_directml: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct LlmInfo { pub context_max: u32, pub context_8gb: u32, pub context_16gb: u32, pub thinks: bool }
#[derive(Debug)] pub struct ModelFile { pub name: &'static str, pub url: &'static str, pub size: u64, pub sha256: &'static str }
#[derive(Debug)] pub struct ModelInfo {   // NOT Clone/Copy/Serialize: always &'static; build your own DTO for Tauri
  pub id: &'static str, pub name: &'static str, pub maker: &'static str, pub note: &'static str, pub kind: Kind,
  pub languages: &'static str, pub english_only: bool, pub speed: u8 /*1..3*/, pub accuracy: u8 /*1..3*/,
  pub licence: Licence, pub files: &'static [ModelFile], pub gpu: GpuSupport,
  pub keeps_fillers: Option<bool>,   // measured only for whisper-turbo & parakeet-ultra (Some(true))
  pub chunk_secs: Option<u32>,       // Some(40) for ONNX models; None = any length (Whisper)
  pub llm: Option<LlmInfo>,
}
impl ModelInfo { pub fn size(&self) -> u64 /*sum of files*/; pub fn is_speech(&self) -> bool }
pub fn all() -> &'static [ModelInfo];
pub fn speech() -> impl Iterator<Item = &'static ModelInfo>;
pub fn local_ai() -> impl Iterator<Item = &'static ModelInfo>;   // kind == Llm only (NOT the "search" Embedding model)
pub fn find(id: &str) -> Option<&'static ModelInfo>;
pub static CATALOG: &[ModelInfo];
// gpu consts: gpu::WHISPER_AND_LLM {metal,vulkan}, gpu::PARAKEET_TDT {directml}, gpu::NONE. licences::{MIT, APACHE_2, CC_BY_4, OPENMDW_1_1}
```
Catalog never recommends a model. Ids (catalog order): id | name | Kind | total bytes | files
```
whisper-turbo | Whisper Large Turbo (compressed) | Whisper | 574,041,195 | ggml-large-v3-turbo-q5_0.bin
whisper-turbo-full | Whisper Large Turbo (full size) | Whisper | 1,624,555,275 | ggml-large-v3-turbo.bin
parakeet-v3 | Parakeet | Parakeet | 670,619,803 | encoder-model.int8.onnx, decoder_joint-model.int8.onnx, nemo128.onnx, vocab.txt, config.json
parakeet-v3-full | Parakeet (full size) | Parakeet | 2,549,945,719 | encoder-model.onnx(+.data), decoder_joint-model.onnx, nemo128.onnx, vocab.txt, config.json
whisper-small | Whisper Small | Whisper | 487,601,967 | ggml-small.bin
whisper-small-en | Whisper Small (English) | Whisper | 487,614,201 | ggml-small.en.bin
whisper-large-v3 | Whisper Large v3 (compressed) | Whisper | 1,081,140,203 | ggml-large-v3-q5_0.bin
whisper-large-v3-full | Whisper Large v3 (full size) | Whisper | 3,095,033,483 | ggml-large-v3.bin
whisper-large-v2 | Whisper Large v2 (compressed) | Whisper | 1,080,732,091 | ggml-large-v2-q5_0.bin
whisper-large-v2-full | Whisper Large v2 (full size) | Whisper | 3,094,623,691 | ggml-large-v2.bin
whisper-medium | Whisper Medium | Whisper | 1,533,763,059 | ggml-medium.bin
whisper-medium-en | Whisper Medium (English) | Whisper | 1,533,774,781 | ggml-medium.en.bin
whisper-base | Whisper Base | Whisper | 147,951,465 | ggml-base.bin
whisper-base-en | Whisper Base (English) | Whisper | 147,964,211 | ggml-base.en.bin
whisper-tiny | Whisper Tiny | Whisper | 77,691,713 | ggml-tiny.bin
whisper-tiny-en | Whisper Tiny (English) | Whisper | 77,704,715 | ggml-tiny.en.bin
parakeet-ultra | Parakeet Ultra | Parakeet | 667,961,292 | (int8 set as parakeet-v3)
parakeet-v2 | Parakeet v2 | Parakeet | 661,331,545 | (int8 set)
parakeet-v2-full | Parakeet v2 (full size) | Parakeet | 2,513,132,330 | (full set)
parakeet-unified-en | Parakeet Unified | ParakeetUnified | 663,344,373 | encoder.int8.onnx(+.data), decoder_joint.int8.onnx, tokenizer.model
parakeet-unified-en-full | Parakeet Unified (full size) | ParakeetUnified | 2,515,023,099 | encoder.onnx(+.data), decoder_joint.onnx, tokenizer.model
nemotron-3.5 | Nemotron 3.5 | Nemotron | 2,594,566,700 | encoder.onnx(+.data), decoder_joint.onnx, tokenizer.model
vad | Silero VAD | Vad | 885,098 | ggml-silero-v6.2.0.bin
diarization | Speaker separation | Diarization | 400,506,656 | nemotron3_diar_v3.onnx
search | Search by meaning | Embedding | 639,150,592 | Qwen3-Embedding-0.6B-Q8_0.gguf
qwen3.5-4b | Qwen3.5 4B | Llm | 2,740,937,888 | Qwen3.5-4B-Q4_K_M.gguf   llm{ctx_max 262144, 8gb 65536, 16gb 131072, thinks true}
qwen3.5-9b | Qwen3.5 9B | Llm | 5,680,522,464 | Qwen3.5-9B-Q4_K_M.gguf   llm{262144, 4096, 131072, thinks true}
granite-4.2-3b | Granite 4.2 3B | Llm | 2,244,011,552 | granite-4.2-3b-Q4_K_M.gguf  llm{131072, 32768, 65536, thinks true}
```
GPU: whisper-* and LLMs = mac_metal+windows_vulkan; parakeet-v2/v3/ultra(+full) = windows_directml only; parakeet-unified, nemotron, vad, diarization, search = none.
(Talaan's legacy list lacks the *-full, *-en, parakeet-*-full, granite ids; those are not auto-adopted from Talaan's folders.)

## 3. voice-engine-stt  (`voice_engine_stt`)

Root exports:
```rust
pub use api::{ChunkOptions, LoadOptions, Segment, SpeechBackend, SpeechError, Timing, Transcriber, Transcript, Word, load};
pub use helper::run_helper;
pub mod diarize; pub mod directml; pub mod parakeet_engine; pub mod threads; pub mod vad; pub mod whisper_engine; pub mod words;
pub fn discrete_card() -> Option<Device>;          // in-process DXGI/DXCore; None without `directml` feature or off Windows
pub fn directml_devices() -> Vec<Device>;
pub fn devices() -> Vec<Device>;                   // ggml devices
pub fn is_noise(text: &str) -> bool;
// reference-only: pub fn transcribe(state:&mut whisper_rs::WhisperState, samples:&[f32], language:Option<&str>, n_threads:usize) -> Result<(Vec<Piece>,Option<String>),String>
```
```rust
pub fn load(model: &ModelInfo, dir: &Path /* the model's folder = ModelHandle::dir() / SharedStore::model_dir */, opts: LoadOptions)
    -> Result<Box<dyn Transcriber>, SpeechError>;     // Whisper + Parakeet/ParakeetUnified/Nemotron in process; other Kinds => Unsupported
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadOptions { pub gpu: bool, pub threads: Option<usize> }   // Default = {gpu:false, threads:None}
pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32] /*16 kHz mono*/, opts: &ChunkOptions) -> Result<Transcript, SpeechError>;
    fn backend(&self) -> SpeechBackend;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechError { ModelMissing, BadModel, OutOfMemory, Unsupported(String), TooLong { max_secs: u32 }, Internal(String) }  // Display + Error
```
Whisper takes any length; ONNX models take <= catalog `chunk_secs` (40 s) per call else `TooLong` (app cuts + offsets times). `gpu:true` where there's no card path => loads on CPU, `backend().fell_back == true`, no error. Mac ONNX is always CPU (fell_back=true). `ChunkOptions.keep_fillers`/`vocabulary` ignored by ONNX models; `language` used only by Nemotron.
These are defined in `voice_engine_protocol::stt` and re-exported (same types):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] #[serde(default)]
pub struct ChunkOptions { #[serde(skip_serializing_if=Option::is_none)] pub language: Option<String>, pub timing: Timing, pub keep_fillers: bool, #[serde(skip_serializing_if=Vec::is_empty)] pub vocabulary: Vec<String> }
//   impl Default: language None, timing Timing::Words, keep_fillers false, vocabulary []
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)] #[serde(rename_all="snake_case")]
pub enum Timing { Segments /* Talaan's settings: model's segments; words derived */, Words /* Ensayo's: one word per segment */ }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transcript { pub segments: Vec<Segment>, pub language: Option<String>, pub stopped_early_at_ms: Option<u64> }   // both Options serde(default, skip_if none)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] pub struct Segment { pub start_ms: u64, pub end_ms: u64, pub text: String, pub words: Vec<Word> }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] pub struct Word { pub text: String, pub start_ms: u64, pub end_ms: u64 }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeechBackend { pub backend: String /* "metal"|"vulkan"|"directml"|"cpu" */, pub device: Option<String>, pub fell_back: bool }
```
Threads (`voice_engine_stt::threads`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ThreadKind { Processor, Gpu }
pub fn default_threads(kind: ThreadKind, logical: usize) -> usize;   // Processor: (logical*3/4).clamp(2,12); Gpu: logical.clamp(2,8)
pub fn resolve_threads(kind: ThreadKind, requested: Option<usize>, logical: usize) -> usize;  // Some(n)=>n.max(1) uncapped
pub fn logical_cores() -> usize;                                      // 4 if unknown
```
VAD (`voice_engine_stt::vad`): `pub fn detect(model_dir: &Path /* the "vad" model folder */, samples: &[f32]) -> Result<Vec<(u64,u64)>, SpeechError>;` spans (start_ms,end_ms) ordered, non-overlapping, always CPU, stateless (reloads <1MB model each call); empty audio / < 512 samples => Ok(vec![]); missing file => ModelMissing. Settings = Talaan's (thr 0.5, min speech 250ms, min silence 300ms, no padding). `pub const FRAME_SAMPLES: usize = 512`.
Diarize (`voice_engine_stt::diarize`):
```rust
pub fn load(model_dir: &Path /* "diarization" model folder */, threads: Option<usize>) -> Result<Diarizer, SpeechError>;
pub struct Diarizer { /* Sortformer, threads */ }  // Send
impl Diarizer { pub fn run(&mut self, track: &[f32] /*16 kHz mono*/) -> Result<Vec<SpeakerFrame>, SpeechError>; pub fn threads(&self) -> usize; }
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeakerFrame { pub start_ms: u64, pub end_ms: u64, pub speakers: [f32; SPEAKERS] }  // probability 0..1 per column; 10 ms frames
pub const FRAME_MS: u64 = 10; HOP_SAMPLES: usize = 160; SPEAKERS: usize = 8; WARMUP_SAMPLES = 10*60*16_000; CHUNK_SAMPLES = 5*60*16_000;
```
Column order has no meaning across calls; naming speakers / frames->turns is the app's job. Always CPU; resets state each run; warm-up on first 10 min.
`Device` type (protocol): see §5. `voice_engine_stt::discrete_card() -> Option<Device>`; also `Device` for `devices()`.
Whisper engine pieces if needed: `whisper_engine::WhisperEngine::load(file:&Path, opts:LoadOptions)->Result<Self,SpeechError>`, `ParakeetEngine::load(model:&ModelInfo, dir:&Path, opts)`; `.threads()`, `ParakeetEngine::max_secs()`. Use `load()` instead.
Helper (`run_helper(config)`): Whisper only; exits when stdin closes; no idle timer; writes nothing to stderr except `<PREFIX>_GPU=fail` line.

## 4. voice-engine-client  (`voice_engine_client`)

Root: `pub use error::EngineError; pub use gpu_memory::clear_gpu_failure; pub use llm::{Limits, LlmClient, LlmConfig, LoadInfo, Outcome}; pub use stt::{SttClient, SttConfig, SttLoadInfo}; pub mod servers;` (+ `#[doc(hidden)] __testing`)
```rust
#[derive(Debug, Clone)] pub struct Limits { pub start: Duration, pub stall: Duration, pub total: Duration }
impl Default for Limits  // start 60s, stall 120s, total 30 min
#[derive(Debug, Clone)]   // NO Default impl
pub struct LlmConfig {
  pub cpu_helper: PathBuf,                    // every platform (Mac: the Metal build)
  pub gpu_helper: Option<PathBuf>,            // Windows Vulkan helper; tried first until it fails once
  pub env: Vec<(String, String)>,             // both helpers, e.g. ("TALAAN_GPU", ..)
  pub gpu_env: Vec<(String, String)>,         // GPU helper only, e.g. VK_LOADER_LAYERS_DISABLE=~implicit~
  pub gpu_failures: Option<PathBuf>,          // file remembering GPU failure
  pub gpu_key: String,                        // app version + driver; change => retry card
  pub idle_secs: Option<u64>,                 // None = runs while app runs
  pub limits: Limits,
}
#[derive(Debug, Clone)] pub struct Outcome<T> { pub value: T, pub backend: Backend, pub fell_back: bool }  // Backend = voice_engine_protocol::Backend; fell_back => show "GPU failed, used processor" once
#[derive(Debug, Clone, PartialEq)] pub struct LoadInfo { pub seconds: f32, pub n_ctx_train: u32 }
impl LlmClient {   // Send+Sync use from any thread via &self; one request at a time
  pub fn new(config: LlmConfig) -> Self;
  pub fn load(&self, model_path: &str, n_ctx: u32, gpu: bool, progress: &mut dyn FnMut(&Event)) -> Result<Outcome<LoadInfo>, EngineError>;   // Event = voice_engine_protocol::Event
  pub fn complete(&self, completion: Completion, progress: &mut dyn FnMut(&Event)) -> Result<Outcome<Reply>, EngineError>;
  pub fn embed(&self, embedding: Embedding) -> Result<Outcome<Vec<Vec<f32>>>, EngineError>;       // L2-normalized
  pub fn inspect(&self, model_path: &str) -> Result<Inspection, EngineError>;
  pub fn devices(&self) -> Result<DeviceList, EngineError>;               // 10 s cap => TimedOut; starts helper
  pub fn discrete_card(&self) -> Result<Option<Device>, EngineError>;     // treat Err as "no card"
  pub fn unload(&self) -> Result<(), EngineError>;                        // no-op if no helper running
  pub fn set_idle(&self, idle_secs: Option<u64>) -> Result<(), EngineError>;
  pub fn cancel(&self);                                                   // no Result; request ends with EngineError::Cancelled; helper killed after 2 s if unresponsive
  pub fn shutdown(&self);                                                 // no Result; kills helper (use before Windows update)
}
// There is no `unload`-by-model; model_path = join(handle.dir(), catalog file name). Progress events: Event::Progress{phase,done,total}.
#[derive(Debug, Clone)]
pub struct SttConfig { pub cpu_helper: PathBuf, pub gpu_helper: Option<PathBuf>, pub env: Vec<(String,String)>, pub gpu_env: Vec<(String,String)>,
                       pub gpu_failures: Option<PathBuf>, pub gpu_key: String, pub limits: Limits }   // no idle_secs; no Default. `stall` must exceed longest Whisper decode (no events during decode)
#[derive(Debug, Clone, PartialEq)] pub struct SttLoadInfo { pub seconds: f32 }
impl SttClient {
  pub fn new(config: SttConfig) -> Self;
  pub fn load(&self, model: SttLoad) -> Result<Outcome<SttLoadInfo>, EngineError>;   // Whisper only; remembered & re-sent to any new helper
  pub fn transcribe(&self, audio: &[f32], options: &ChunkOptions) -> Result<Outcome<Transcript>, EngineError>;  // audio sent base64: cut long recordings (~300MB/h)
  pub fn devices(&self) -> Result<DeviceList, EngineError>;
  pub fn discrete_card(&self) -> Result<Option<Device>, EngineError>;
  pub fn unload(&self) -> Result<(), EngineError>;     // also forgets the remembered model
  pub fn shutdown(&self);
  pub fn cancel(&self);                                // call ends 2 s later by killing helper; next call reloads model
}
pub fn clear_gpu_failure(path: &Path);                 // app's "try the graphics card again"
```
SttLoad (protocol::stt): `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)] pub struct SttLoad { pub model_id: String /* catalog id */, pub model_dir: String, pub kind: SttModelKind, pub gpu: bool /*default true*/, pub threads: Option<usize> }`; `pub enum SttModelKind { Whisper, Parakeet, ParakeetUnified, Nemotron }` (snake_case serde; helper serves Whisper only).
Protocol `Completion` (voice_engine_protocol::Completion) — Debug, Clone, Serialize, Deserialize, PartialEq; NO Default:
```rust
pub struct Completion { pub model_path: String, pub n_ctx: u32, pub system: String, pub user: String,
  pub json_schema: Option<String>,        // skip if None; constrains reply to schema (JSON_OBJECT_SCHEMA = r#"{"type":"object"}"#)
  pub max_tokens: u32, pub temperature: f32, pub gpu: bool /*serde default true; set explicitly in Rust*/,
  pub thinking: bool /*default false*/, pub thinking_budget: Option<u32>, pub seed: Option<u32> /*default 1234 when temp>0*/ }
pub struct Embedding { pub model_path: String, pub texts: Vec<String>, pub gpu: bool }   // Debug, Clone, Serialize, Deserialize, PartialEq
pub struct Reply { pub text: String, pub prompt_tokens: u32, pub output_tokens: u32, pub seconds: f32, pub truncated: bool,
  pub thinking_tokens: u32, pub load_seconds: f32, pub thinking_text: Option<String>, #[serde(flatten)] pub backend: Backend }  // Debug, Clone, Serialize, Deserialize, PartialEq
pub struct Inspection { pub has_chat_template: bool, pub thinks: bool, pub n_ctx_train: u32, pub architecture: String }  // Debug, Clone, Serialize, Deserialize, PartialEq, Eq
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Backend { pub backend: String /*"vulkan"|"metal"|"cpu"*/, pub device: Option<String>, pub vram_mb: u64 }   impl Backend { pub fn cpu()->Self; pub fn on_gpu(&self)->bool }
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)] pub enum Phase { Loading, Reading, Writing }
pub enum Event { Ready{backend,protocol}, Progress{phase:Phase,done:u32,total:u32}, Done(Reply), Embedded{vectors:Vec<Vec<f32>>,backend}, Devices{devices:Vec<Device>,chosen:Option<usize>}, Configured{idle_secs:Option<u64>}, Unloaded, Loaded{seconds:f32,n_ctx_train:u32,backend}, Inspected(Inspection), Error{message:String,error:ErrorKind} }  // Debug, Clone, Serialize, Deserialize, PartialEq; tag="event"
```
EngineError (`#[derive(Debug, Clone, PartialEq)]`, Display = `{:?}` for logs, impl std::error::Error; NOT Serialize):
```rust
pub enum EngineError {
  ModelMissing { path: String }, OutOfMemory, PromptTooLong { prompt_tokens: u32, n_ctx: u32, max_tokens: u32 },
  BadSchema { detail: String }, NoChatTemplate, BadModel, TooLong { max_secs: u32 }, Unsupported { detail: String },
  Cancelled, GpuFailed { fell_back: bool }, BadRequest, Internal { message: String },
  Crashed, VersionMismatch { helper: u32, app: u32 }, TimedOut, CouldNotStart { message: String },
}
// impl EngineError { pub fn from_helper(ErrorKind, String)->Self; pub fn from_stt_helper(SttErrorKind, String)->Self }
```
servers (`voice_engine_client::servers`; BLOCKING, call from worker thread/spawn_blocking):
```rust
pub fn detect() -> Vec<LocalServer>;                       // Ollama :11434 then LM Studio :1234; absent servers omitted
pub fn detect_at(probes: &[Probe]) -> Vec<LocalServer>;
pub fn default_probes() -> Vec<Probe>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ServerKind { Ollama, LmStudio }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ServerState { Running, Unreadable }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ServerModel { pub id: String, pub context_length: Option<u32>, pub size_bytes: Option<u64> }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct LocalServer { pub kind: ServerKind, pub base_url: String, pub api_base: String /*base+"/v1"*/, pub state: ServerState, pub models: Vec<ServerModel> }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Probe { pub kind: ServerKind, pub base_url: String }
```
(300 ms connect, 2 s per server, parallel; none are Serialize.)
GPU-fallback rule: if `gpu_helper` set and it fails, the CPU helper gets the same request with `gpu:false`. Metal helper death counts as a card failure. On Mac set gpu_helper=None, cpu_helper = the Metal sidecar.

## 5. voice-engine-protocol  (`voice_engine_protocol`)
```rust
pub const PROTOCOL_VERSION: u32 = 2;
pub const JSON_OBJECT_SCHEMA: &str = r#"{"type":"object"}"#;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct HelperConfig { pub env_prefix: &'static str }   // impl: pub fn gpu_env(&self)->String ("<P>_GPU"); pub fn idle_secs_env(&self)->String ("<P>_LLM_IDLE_SECS")
pub use gpu::{Device, DeviceKind, DeviceList};   pub mod config; pub mod gpu; pub mod stt;
// fns: encode<T: Serialize>(&T)->String; decode_request(&str)->Result<Request,serde_json::Error>; decode_event(&str)->Result<Event,_>
// types also: Request (op: ping|configure|load|complete|cancel|unload|embed|inspect|devices), Configure{idle_secs:Option<u64>}, Load{model_path,n_ctx,gpu}, Inspect{model_path}, ErrorKind (tag "kind": ModelMissing{path},OutOfMemory,PromptTooLong{..},BadSchema{detail},NoChatTemplate,Cancelled,GpuFailed{fell_back},BadRequest,Internal[default])
// stt mod: SttRequest, SttEvent, SttErrorKind{ModelMissing{path},BadModel,OutOfMemory,TooLong{max_secs},Unsupported{detail},GpuFailed{fell_back},Cancelled,BadRequest,Internal}, encode_samples(&[f32])->String, decode_samples(&str)->Result<Vec<f32>,String>
```
gpu module:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)] #[serde(rename_all="snake_case")]
pub enum DeviceKind { Cpu, Gpu /*discrete*/, Igpu, Accel, Unknown }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device { pub index: usize, pub name: String /*"Vulkan0"*/, pub description: String /*"NVIDIA GeForce RTX 5070 Ti"*/, pub backend: String /*"Vulkan","CPU","DirectML"*/, pub kind: DeviceKind, pub memory_mb: u64 }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeviceList { pub devices: Vec<Device>, #[serde(default, skip_serializing_if="Option::is_none")] pub chosen: Option<usize> }
impl DeviceList { pub fn discrete_card(&self) -> Option<&Device> }
pub const DIRECTML: &str = "DirectML";
pub fn pick(devices:&[Device])->Option<&Device>; pub fn discrete_card(devices:&[Device])->Option<&Device>;  // discrete GPU w/ most memory, never iGPU, 0 MB => not a card
pub fn whisper_gpu_index(devices:&[Device], chosen:&Device)->Option<i32>; pub fn bytes_to_mb(usize)->u64;
```
Windows default rule: card on by default only when discrete_card is Some (stt in-process `discrete_card()` w/ directml feature, else `SttClient/LlmClient::discrete_card()`; treat Err as no card).

## 6. README / docs quotes

### "Build settings an app must copy" — exact block (engine `.cargo/config.toml`; app copies to `src-tauri/.cargo/config.toml`, COMMITTED, or sets same env in release workflow):
```toml
[env]
GGML_NATIVE = { value = "OFF", force = true }            # no "optimize for this CPU"; read by whisper-rs-sys and llama-cpp-sys-2
GGML_AVX = { value = "ON", force = true }                # the x86 baseline: AVX2 + FMA + F16C + BMI2 (2013 and newer)
GGML_AVX2 = { value = "ON", force = true }
GGML_FMA = { value = "ON", force = true }
GGML_F16C = { value = "ON", force = true }
GGML_BMI2 = { value = "ON", force = true }
MACOSX_DEPLOYMENT_TARGET = { value = "11.0", force = true }   # the first macOS for Apple silicon; read by cc, cmake and rustc
CMAKE_C_FLAGS_RELEASE = { value = "-O2 -DNDEBUG", force = true }     # optimization: cmake-rs strips /O under the Visual Studio generator
CMAKE_CXX_FLAGS_RELEASE = { value = "-O2 -DNDEBUG", force = true }   # (Windows x64 builds at /Od, 10x slower); accepted by cl, clang-cl and clang

[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+avx2,+fma,+f16c,+bmi2"]   # llama-cpp-sys-2 turns these into GGML_AVX2, GGML_FMA, ...
```
Key points: Cargo reads `.cargo/config.toml` only for the workspace it builds => a git dependency gets NONE of it, silently. Applies to any app that links voice-engine-llm OR voice-engine-stt (helpers OR in-process speech crate; the reusable workflow covers only the helpers, NOT in-process stt). The two CMAKE_*_FLAGS_RELEASE lines are mandatory on Windows (else /Od, Whisper CPU decode never finishes). After editing: `cargo clean --release -p whisper-rs-sys -p llama-cpp-sys-2` (build scripts don't rerun on env change). `RUSTFLAGS` env REPLACES the rustflags line (put target-feature in it too). Windows ARM64 (`aarch64-pc-windows-msvc`): clang-cl + Ninja (`CC`/`CXX`=clang-cl, `CMAKE_GENERATOR=Ninja`, `CXXFLAGS=/EHsc`, VS ARM64 env; see `.github/actions/windows-arm64-toolchain`); no ARM64 lines to copy. App should fail its release if CMake caches lack `GGML_NATIVE:BOOL=OFF` / `-O2` (see scripts/check-baseline.sh).

### "What an app calls" — at start, in this order
```rust
let store = SharedStore::open(App::Talaan)?;      // creates the folder and .engine if they are not there
let report = store.move_in(&["whisper-turbo"])?;  // first thing, once; `used` = model ids the settings point to
store.collect()?;                                 // erases models nobody uses and old part files
let everything = store.list();                    // every catalog model with its Status
```
`download(id,&cancel,&mut progress)` returns model dir; `DownloadingElsewhere` => show other app's progress from `status`. `use_model(id)` = marker only (model already present). `hold(id)` -> keep handle while loaded, drop on unload. Delete = `release(id)` (never remove folders yourself); Delete all data = `release_all()`. Tests/dev: `open_at` with own dir; `read_only(app,dir)` for "what's on this computer" screens.

### "The one-time move" — see §2 (move_in semantics). Writes `.engine/moved/<app>` last; safe to rerun.

### docs/building-helpers.md
Reusable workflow: `mmnzns/voice-engine/.github/workflows/build-helpers.yml@v0.2.2` (`on: workflow_call` + `workflow_dispatch`). Use same tag for `@tag` and `engine-ref`. It checks out the ENGINE at engine-ref and builds there (so engine's .cargo/config.toml applies), stable Rust, `cargo build --release --locked`. `permissions: contents: read`. NO secrets required, NO workflow-level outputs (artifacts only). Runners: macos-latest (aarch64-apple-darwin), windows-latest (x86_64-pc-windows-msvc), windows-11-arm (aarch64-pc-windows-msvc).
Inputs: `engine-ref` (string, required; in workflow_dispatch default '' = the commit run was started on) | `helpers` (default `llm,stt`; comma list among llm, stt, llm-vulkan, stt-vulkan; Vulkan = x64 Windows only, others skip) | `llm-name` (default `voice-engine-llm`) | `stt-name` (default `voice-engine-stt`) | `cache` (boolean, default true).
Caller example (verbatim):
```yaml
jobs:
  helpers:
    uses: mmnzns/voice-engine/.github/workflows/build-helpers.yml@v0.2.2
    with:
      engine-ref: v0.2.2
      helpers: llm,stt                 # add llm-vulkan,stt-vulkan for the Windows x64 graphics card builds
      llm-name: ensayo-llm
      stt-name: ensayo-stt
  app:
    needs: helpers
    strategy:
      fail-fast: false
      matrix:
        include:
          - { os: macos-latest, triple: aarch64-apple-darwin }
          - { os: windows-latest, triple: x86_64-pc-windows-msvc }
          - { os: windows-11-arm, triple: aarch64-pc-windows-msvc }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: helpers-${{ matrix.triple }}
          path: src-tauri/binaries
      # A downloaded artifact does not keep the executable bit.
      - name: Make the helpers executable
        if: runner.os == 'macOS'
        run: chmod +x src-tauri/binaries/*-apple-darwin
```
Outputs: one artifact per triple named `helpers-<target triple>` containing the helpers `<name>-<triple>[.exe]`, `SHA256SUMS`, `build-info.txt` (engine ref+commit, target, rustc, protocol version, pinned llama-cpp-2/-sys-2, whisper-rs/-sys, parakeet-rs, ort/ort-sys versions). Delete SHA256SUMS/build-info.txt from src-tauri/binaries if unwanted; Tauri bundles only `externalBin` entries.
File names for llm-name=talaan-llm, stt-name=talaan-stt (pattern per docs): 
- aarch64-apple-darwin: `talaan-llm-aarch64-apple-darwin`, `talaan-stt-aarch64-apple-darwin`
- x86_64-pc-windows-msvc: `talaan-llm-x86_64-pc-windows-msvc.exe`, `talaan-stt-x86_64-pc-windows-msvc.exe` (+ Vulkan: `talaan-llm-vulkan-x86_64-pc-windows-msvc.exe`, `talaan-stt-vulkan-x86_64-pc-windows-msvc.exe`)
- aarch64-pc-windows-msvc: `talaan-llm-aarch64-pc-windows-msvc.exe`, `talaan-stt-aarch64-pc-windows-msvc.exe`
(Vulkan name = `<name>-vulkan-<triple>.exe`. Mac has no Vulkan; Mac helpers use Metal.) NO x86_64-apple-darwin target.
Tauri (verbatim): `"bundle": { "externalBin": ["binaries/ensayo-llm", "binaries/ensayo-stt"] }` — list WITHOUT triple; Tauri finds `src-tauri/binaries/<name>-<triple>[.exe]` and installs it next to the app exe without the triple. Every externalBin entry must exist for the target being built => list the `-vulkan` entries only in a Windows-x64-specific config merged via `--config`/platform file.
Closing helpers before update (Windows): installer can't replace a running exe; helpers run as long as the app (exit when stdin closes). An app whose Windows installer replaces helper files must call `shutdown()` on `LlmClient` and `SttClient`, or have the installer kill the processes by name (names = llm-name/stt-name; Vulkan = `<name>-vulkan`, same extension). Otherwise "file in use" or old helper alongside new app.
Signing: engine ships UNSIGNED. Mac: sign every sidecar inside the .app with the app's identity + hardened runtime (an unsigned helper in a signed app isn't covered). Windows: sign each .exe (app + every helper incl. Vulkan) with the app's cert. Apps are self-signed/not notarized. Signing changes the file so SHA256SUMS describes the unsigned build. Docs: v2.tauri.app/distribute/sign/{macos,windows}/.
By hand: `scripts/build-helpers.sh aarch64-apple-darwin dist --engine-ref v0.2.2`; `scripts/build-helpers.ps1 -Target x86_64-pc-windows-msvc -OutDir dist -EngineRef v0.2.2`. Handshake check: stdin `{"op":"ping"}` -> stdout `{"event":"ready","backend":"cpu","vram_mb":0,"protocol":2}`.
DirectML is NOT in helpers (both Vulkan helpers = `--features vulkan` only; stt helper = Whisper only). App enables `voice-engine-stt/directml` itself for Parakeet on the card.
Helpers never log user text; "Which app uses which tag": Talaan "moving to v0.2.2".
