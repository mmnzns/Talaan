# M4. Transcriber — task plan

> Generated from Notion "Talaan v1 Build Plan" › M4. Model: **Opus**.
> No 🛑 at the end of M4: the next checkpoint is M5 (first real summary). M4 is checked on the real 2:31 PM Zoom recording.
> Done when: stopping a recording produces a Me / Them transcript with timestamps on its own; echo lines from the speakers are gone; Recover and Try again transcribe; a model downloads from Settings with progress; the app stays usable while it works.

## Verified facts (2026-10-14)

- No speech models exist on this Mac (searched `~` for ggml / onnx / gguf over 20 MB). Talaan must download its own. Parlato on a Mac would keep them in `~/Library/Application Support/com.craftconceptsdigital.parlato/` (`ParakeetModels/<id>/`, `Models/<id>.bin`); that folder doesn't exist here. VoiceInk's folder has no models. So "On this Mac" reuse ships, but can only be tested with a fake folder.
- Models (Hugging Face API, sizes from the LFS metadata, which also gives sha256):

| id | Files | Size | License | Languages |
|---|---|---|---|---|
| `parakeet-v3` (default) | `istupakov/parakeet-tdt-0.6b-v3-onnx`: `encoder-model.int8.onnx`, `decoder_joint-model.int8.onnx`, `nemo128.onnx`, `vocab.txt`, `config.json` | 670 MB | CC-BY-4.0 | 25 European |
| `whisper-turbo` | `ggerganov/whisper.cpp`: `ggml-large-v3-turbo-q5_0.bin` | 574 MB | MIT | 99 (incl. Filipino) |
| `whisper-small` | `ggerganov/whisper.cpp`: `ggml-small.bin` | 488 MB | MIT | 99 |
| VAD (always) | `ggml-org/whisper-vad`: `ggml-silero-v6.2.0.bin` | 1 MB | MIT | — |

- **Nemotron** (`altunenes/parakeet-rs` › `nemotron-3.5-asr-streaming-0.6b-onnx`, 2.6 GB) has no license on its card. Task 9 checks NVIDIA's license for the base model; if it isn't clearly redistributable / usable, Nemotron is hidden in v1 and the decision goes to the Build Log.
- Crates (all in the local registry already): `parakeet-rs` 0.3.8 (MIT/Apache, TDT with sentence and word timestamps; also has Sortformer diarization for M8), `whisper-rs` 0.16 (Unlicense, `metal` feature, standalone `WhisperVadContext`), `reqwest` 0.12, `sha2`, `strsim`. Text language detection: `whatlang` (MIT), verify on add.

## Architecture decisions (Opus, binding)

### Pipeline per meeting

```
me.wav ──┐                     ┌─ VAD segments ─ engine ─ lines (speaker me)  ─┐
         ├─ load 16 kHz mono ──┤                                              ├─ merge ─ echo dedupe ─ transcript.json/.md
them.wav ┘                     └─ VAD segments ─ engine ─ lines (speaker them:0)┘
```

1. **VAD first** (Silero via `WhisperVadContext`) on each track: speech segments, padded 200 ms, merged when the gap is under 0.5 s, split at 25 s max. Silence never reaches the engine, which removes the classic "Thank you." hallucinations and makes long quiet calls cheap.
2. **Engine** behind a trait in `src-tauri/src/transcriber/engine.rs`: `fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> Result<Vec<Segment>>` where `Segment { start_ms, end_ms, text }` relative to the chunk. Implementations: `ParakeetEngine` (sentence timestamps) and `WhisperEngine` (segment timestamps, Metal on). Chunk offsets are added by the pipeline, so positions are on the recording clock (which already excludes pauses, matching marks and pause gaps).
3. **Merge** (pure, TDD): sort by start; join consecutive same-speaker segments when the gap is < 1.2 s and the joined line stays under ~40 s; stable ids `l0..`.
4. **Echo dedupe** (pure, TDD): drop a Me line when a Them line overlaps it in time (±2 s) and their normalized texts are similar (token-set similarity ≥ 0.6, or the Me text is mostly contained in the Them text). Partial echo inside a longer Me line is left alone (keeping real speech beats removing echo). Counted and logged in `meeting.json` (`echoLinesRemoved`) so the M4 check can report it. Tested on fixtures and then measured on the real 2:31 PM recording.
5. **Language:** setting `meetingLanguage` (`auto` or a code). Whisper: pass it or let Whisper detect. Parakeet: no language input; detect from the transcript text with `whatlang`. The meeting's `language` is set either way. If Parakeet is active and the detected language isn't one of its 25, the meeting gets a notice "Transcribed with Parakeet, which doesn't support <lang>. Try again with Whisper." (button in M7; text only now).

### Jobs and status

- One background worker thread, one job at a time, FIFO queue in memory, rebuilt on launch from meetings whose status is `transcribing` (crash during transcription just restarts it).
- `stop` → status `transcribing`, progress 0 → worker → writes `transcript.json` (original, never rewritten) + `transcript.md` → status `ready` (M5 will go to `summarizing`). Progress = processed speech seconds / total speech seconds across both tracks, written to `meeting.json` at most every 2 s and emitted as `meetings-changed`.
- `recover_meeting` (M3) and `retry_processing` now queue a job instead of returning `not_yet_available`.
- Failure (model missing, decode error): status `failed` with `error` text; Try again re-queues. **No model downloaded:** status `failed`, error "Download a speech model in Settings › Speech models to transcribe this call.", and when a model finishes downloading, every meeting failed for that reason is re-queued automatically.
- Only the **active model** is loaded, lazily, and dropped after 5 minutes idle (memory on a laptop matters more than a 2 s load).
- Recording and transcribing at once is allowed (a second call can start while the first transcribes); the worker runs at lower thread priority (`QOS_CLASS_UTILITY` on macOS via `platform/macos`).

### Models

- `src-tauri/src/transcriber/models.rs`: a static catalog (id, engine, files with size and sha256, languages, speed/accuracy hints). The frontend list (`speechModels`) is generated from it, so the mock and the real app can't drift.
- Downloads with `reqwest` (rustls) into `models/<id>/` under the data dir, as `*.part` with HTTP Range resume, sha256 check, then rename. Progress events `model-download` {id, progress}. Cancel supported. The VAD file is fetched with the first model.
- **Reuse ("On this Mac"):** a list of known folders (Parlato's Mac paths above) checked at startup and on Settings open; a model counts only if every file exists with the exact size. Reused models are read in place, never written. The UI never names the other app. Tested with a temp folder through an env override.
- Settings becomes partly real: `speechModels` state (`not_downloaded` / `downloading` + progress / `downloaded` / `on_this_mac`), `activeSpeechModel`, `downloadModel`, `deleteModel` (only Talaan's own copy), `meetingLanguage`, `useGpu` (Whisper Metal on/off).

### Transcript files

`transcript.json` uses the M1/M2 `TranscriptLine` shape (`id, at, end, speaker, text, edited:false`) plus a header `{ schema, engine, model, language, createdAt, echoLinesRemoved }`. `transcript.md` is regenerated by the existing markdown module. Edits stay in `transcript.edits.json` (M2).

## Tasks

TDD for every pure piece (segments math, merge, echo dedupe, progress, catalog, reuse detection, queue rebuild). Engines get an ignored smoke test on a real model. Commit after each task (`M4:`), suites green.

1. **Crates + notices:** `whisper-rs` (`metal`), `parakeet-rs`, `reqwest` (rustls, stream), `sha2`, `strsim` or own similarity, `whatlang`. Verify every license; THIRD_PARTY_NOTICES (libraries and the three models with their licenses; CC-BY-4.0 needs attribution to NVIDIA).
2. **Model catalog + downloader** (`transcriber/models.rs`, `download.rs`): catalog, paths, resume, sha256, cancel, reuse detection. Tests with a local HTTP fixture server (`tiny_http` dev-dependency) and temp dirs.
3. **Audio load + VAD segments** (`transcriber/vad.rs`): read the WAVs, segment math (pad, merge, split) as pure functions; VAD wrapper. Smoke test on the 2:31 PM recording (ignored test reading `TALAAN_SMOKE_MEETING`).
4. **Engines** (`engine.rs`, `parakeet.rs`, `whisper.rs`): trait + two implementations. Smoke tests: speed (real-time factor) on 60 s of the Zoom call for each model; write numbers into the plan Outcome.
5. **Merge + echo dedupe** (`merge.rs`, `echo.rs`): pure, fixture-driven tests (echo identical, echo with ASR differences, real overlap that must stay, partial echo kept).
6. **Pipeline + language** (`pipeline.rs`): tracks → segments → engine → merge → dedupe → language → `TranscriptFile`; progress callback. Pure parts tested with a fake engine.
7. **Worker + status** (`transcriber/worker.rs`, `commands/transcriber.rs`): queue, rebuild on launch, stop → transcribing, recover / retry / auto re-queue after download, idle unload, utility QoS, events. Storage gets `write_transcript` (original, once) and `set_progress`.
8. **Settings + frontend:** real speech models in the Tauri SettingsStore (download with progress, delete, active model, language, GPU); meeting list and banner show real progress; "Download a speech model" failure state links to Settings › Speech models. Mock keeps working in the browser.
9. **Nemotron decision:** read NVIDIA's license for the base model; add to the catalog or hide it, Build Log either way.
10. **Real check:** download Parakeet v3 and Whisper turbo in the signed app; transcribe the 2:31 PM Zoom recording with both (Try again); measure time, echo lines removed, read the transcript for quality and wrong speaker labels; a fresh short recording end to end; force-quit during transcription → relaunch resumes. Outcome in this file, Build Log entry, push.

## Out of scope for M4

Summaries (M5), named speakers / Sortformer (M8), import audio (M7), "Try again with Whisper" button (M7), echo cancellation on the mic (v1.1).

## Outcome (2026-10-14)

Tasks 1–9 done; task 10 checked on the signed debug app except the UI click-through (the screen was locked). 161 Rust tests (8 ignored smoke tests), 73 TS tests.

Measured on the 3.5-minute Zoom recording (M3 Pro, both tracks, ~4 min of detected speech):

| Model | Time | Notes |
|---|---|---|
| Whisper Large Turbo (q5, Metal + flash attention) | 11.7 s (~13x real time per track) | Complete, punctuated, numbers as digits |
| Parakeet v3 (int8, CPU) | ~7 s (~40x) | Skipped stretches and misheard more ("Ta la an", "How did it need to be that") |

Decisions and findings:

- **Default model is Whisper Turbo**, not Parakeet: more accurate and complete on a real call, covers Filipino, same download size (575 MB). An hour-long call takes a few minutes. Parakeet stays as the fast option.
- **Parakeet quirks** (parakeet-rs 0.3.8 + ONNX): it can stop decoding partway through a chunk or return nothing when audio starts right on speech. Fixes: 0.5 s of silence on both sides of every chunk, resume from the last word when output stops more than 2 s early, and a pipeline-level retry of any detected speech of 1 s+ that got no words (applies to every engine).
- **Whisper speed:** one reused state, flash attention, and chunks packed up to its 30 s window (5 calls instead of 13) took it from 4x to 13x real time.
- **Echo removal** dropped 14 echoed Me lines on that call (Miguel was on the speakers); the transcript reads like the real conversation. The test was both ends in one room, so some leak went the other way too.
- **Language** is settled once on the longest chunk (Whisper), or detected from the text (Parakeet), so a meeting never switches language halfway.
- **No models were on this Mac**; downloads verified by sha256 (1.25 GB in 51 s). Reuse looks in Parlato's Mac folders by exact file name and size.
- **Nemotron hidden in v1.** Its license (OpenMDW-1.1) is permissive, but it's a 2.6 GB streaming model, the ONNX export is an undocumented third-party one, and Whisper Turbo already beats Parakeet. Revisit in v1.1.
- **ort pinned to 2.0.0-rc.13**: parakeet-rs 0.3.8 doesn't build against rc.14.
- **Debug builds optimize dependencies** (`[profile.dev.package."*"] opt-level = 3`), otherwise whisper.cpp and the mel features are 10x slower in the signed debug app.
- Crash during transcription: `kill -9` at 22% → relaunch resumes and finishes.
- Fixed along the way: recorder and retry errors showed "Coming in a later build" (new `refused` error code).

Left for Miguel: a fresh call end to end in the app (Stop → progress in the list → transcript), the Settings › Speech models tab (download, cancel, delete).
