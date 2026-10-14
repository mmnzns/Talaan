# M5. Summarizer — task plan

> Generated from Notion "Talaan v1 Build Plan" › M5. Model: **Opus** (Miguel asked to stay on Opus).
> Ends at a 🛑 checkpoint: Miguel's first real meeting summary.
> Done when: a full meeting runs end to end with Wi-Fi off; a 2-hour meeting summarizes; a BYOK run works; malformed model output is handled.

## Verified facts (2026-10-14)

- This Mac: M3 Pro, 18 GB RAM, Metal.
- **ggml conflict.** `whisper-rs-sys` (M4) and `llama-cpp-sys-2` each bundle their own ggml. Linked into one binary, only one copy of each symbol survives. Parlato learned this the hard way (its CLAUDE.md, 2026-09-29): llama-cpp-2 0.1.157 made every model abort, so Parlato is stuck on 0.1.143, which can't run Qwen 3.5 or Gemma 4.
- Candidate local models (Hugging Face API, Q4_K_M GGUF, licenses on the model cards):

| Model | File | Size | License |
|---|---|---|---|
| Qwen3.5 4B | `unsloth/Qwen3.5-4B-GGUF` | 2.74 GB | Apache-2.0 |
| Phi-4-mini instruct | `unsloth/Phi-4-mini-instruct-GGUF` | 2.49 GB | MIT |
| Gemma 4 E4B instruct | `unsloth/gemma-4-E4B-it-GGUF` | 4.98 GB | Apache-2.0 |
| Qwen3.5 9B ("larger" option) | `unsloth/Qwen3.5-9B-GGUF` | 5.68 GB | Apache-2.0 |

- `llama-cpp-2` 0.1.158 (2026-09-30, MIT/Apache) exposes `json_schema_to_grammar`, grammar sampling and `apply_chat_template`. 0.1.159 exists (2026-10-07); take the newest one that passes the smoke test.

## Architecture decisions (Opus, binding)

### The local AI runs in its own process

A small helper binary, `talaan-llm`, bundled inside the app (Tauri `externalBin`), owns llama.cpp. The app starts it when a summary is needed and talks to it over stdin/stdout (one JSON request, streamed JSON progress lines, one JSON result). It exits after 2 minutes idle.

Why: no ggml clash with Whisper (each process has its own copy), so Talaan can use the newest llama.cpp and the best small models; a crash or out-of-memory in the model can't take the app or a recording down; quitting frees every byte of model memory. Cost: one more binary to build and sign (the build script and CI handle it).

- Workspace member `src-tauri/llm/` (crate `talaan-llm`), Mac: `llama-cpp-2` with `metal`. Its protocol types live in a tiny shared crate `src-tauri/llm-protocol/` used by both sides.
- Request: `{ model_path, n_ctx, system, user, json_schema, max_tokens, temperature }`. The helper applies the model's chat template, constrains output with the grammar from `json_schema`, and returns `{ text, prompt_tokens, output_tokens, seconds }`.
- `build.rs`/`scripts`: `cargo build -p talaan-llm` copies the binary to `src-tauri/binaries/talaan-llm-<target-triple>` before `tauri dev/build` (`beforeDevCommand` / `beforeBuildCommand`).

### One summarizer, many engines

`src-tauri/src/summarizer/` (no Tauri types):

- `engine.rs`: `trait SummaryEngine { fn complete(&mut self, req: &Prompt) -> Result<String>; fn context_tokens(&self) -> usize; fn label(&self) -> String }`.
- Engines: `local` (the helper), `ollama` (`/api/chat` with `format: <schema>`), `lmstudio` (OpenAI-compatible `/v1/chat/completions` with `response_format: json_schema`), `claude` (Messages API), `openai` (Chat Completions, `json_schema`), `gemini` (`generateContent` with `responseSchema`), `openai_compatible` (custom base URL). HTTP via the `reqwest` already in the tree. Request building and response parsing are pure and unit-tested with recorded fixtures; the network call is one thin function.
- **Fixed prompt and schema** (`prompt.rs`, the same for every engine, no user editing): Recap (3–5 sentences), Decisions with time, Action items with owner, due date and time, Open questions, plus a short title. The transcript goes in as `[mm:ss] Speaker: text` lines with real speaker names; marks and notes are listed first as "Moments marked during the call" with an instruction to give them priority. Summary language: `summaryLanguage` setting ("meeting" = the meeting's language).
- **Output parsing** (`parse.rs`, TDD): strip code fences, take the outermost JSON object, validate against the schema, convert `"mm:ss"` times to ms and snap each to the nearest transcript line start (a time that matches no line is dropped, never invented), trim empty items, cap list lengths. Bad output → one retry with the parse error appended ("Return only JSON matching the schema"); still bad → the meeting is saved transcript-only with a "Generate summary" button and the reason.
- **Long meetings** (`chunk.rs`, TDD): token estimate per line (chars/3.5, conservative); if the transcript + prompt exceed ~60% of the engine's context, split on speaker turns into parts that fit, summarize each part with the same schema, then a merge call combines the part summaries (dedupe decisions and action items, keep the earliest time). Progress = parts done / (parts + 1).
- **Title:** when the title is still the placeholder, it becomes `<summary title>` (the app and date are already shown beside it). Never overwrites a title the user typed.

### Status flow and jobs

- Transcriber (M4) ends with `summarizing` instead of `ready` when a summary engine is configured and available; otherwise `ready` (transcript-only).
- A second worker thread, same pattern as M4: queue rebuilt from `summarizing` at launch, progress in `meeting.json`, errors kept. `meetings_generate_summary` queues a meeting (also used to regenerate). Writes `summary.json` (original, never rewritten) and `summary.md`; a regenerate replaces the original and deletes `summary.edited.json` only after asking in the UI.
- "No engine available" (local model not downloaded, Ollama not running, no key) → `ready`, transcript-only, with `summaryError` explaining what to set up; when the engine becomes available nothing runs by itself (the user clicks Generate summary), except right after the local model finishes downloading for meetings that were waiting for it.

### Keys and settings

- API keys in the macOS Keychain via the `keyring` crate (`apple-native`), service `Talaan`, account = provider. `settings.json` keeps only `byokKeySet`. Commands `keys_set`, `keys_test` (a 1-token request), `keys_clear`. "Delete all Talaan data" also removes the keys.
- Local models get a catalog like speech models (pinned revisions, sizes, sha256, download/cancel/delete), shown in Settings › Summary AI as "Recommended" and "Larger".

### "Summary ready" notification

`platform/macos/notify.rs`: `UNUserNotificationCenter` (objc2-user-notifications), permission asked the first time a summary finishes (not at launch), click → bring the main window forward and open the meeting. Setting `notifySummaryReady`.

## Tasks

TDD for prompt building, parsing, time snapping, chunking, merge, engine request/response mapping, status flow. The helper and engines get ignored smoke tests. Commit after each task (`M5:`).

1. **Helper process:** workspace, `llm-protocol`, `talaan-llm` with llama-cpp-2 (newest that passes) + Metal; request/response over stdio; idle exit; client in the app that spawns, streams, cancels and restarts it. Smoke test: load a GGUF, constrained JSON output. Verify the app binary still has exactly one ggml (whisper's) and the helper its own.
2. **Model bake-off:** download the three candidates (+ Qwen3.5 9B), run the fixed prompt on the 2:31 PM Zoom transcript and on a long synthetic meeting transcript; measure load time, tokens/s, peak memory, JSON validity, and read the summaries. Pick "Recommended" and "Larger"; numbers go in the plan Outcome and the Build Log. Licenses into THIRD_PARTY_NOTICES.
3. **Prompt, schema, parse, snap** (`prompt.rs`, `parse.rs`).
4. **Chunk and merge** (`chunk.rs`).
5. **Engines:** local (helper), Ollama, LM Studio, Claude, OpenAI, Gemini, OpenAI-compatible; request/response fixtures; availability checks (`is_available`).
6. **Keychain + keys commands**, Test button wired for real.
7. **Summary worker + status flow:** transcriber → summarizing; queue, progress, retry on bad output, transcript-only fallback, Generate summary, title from summary, local-model download re-queue.
8. **Notification** with click-to-open.
9. **Frontend:** Summary AI tab real (local models download, Ollama/LM Studio detection and model pick, provider + key + Test, summary language), meeting view shows the real summary, Generate summary / regenerate, summaryError text.
10. **Done checks:** Wi-Fi off end to end (recording → transcript → local summary); 2-hour meeting summarizes (chunked); BYOK run (needs Miguel's key, or skip and say so); malformed output fixture; then 🛑 for Miguel's first real meeting summary.

## Out of scope for M5

Ask-anything (M8, reuses the engines), named speakers (M8), Notion export (M9).

## Outcome (2026-10-14)

All tasks done. 197 Rust tests (10 ignored smoke tests), 74 TS tests.

**Local model bake-off** (M3 Pro, 18 GB; fixed prompt and schema; a 9-minute planning-call fixture with known answers, `fixtures/summary.planning-call.json`):

| Model (Q4_K_M) | Time | Result |
|---|---|---|
| **Qwen3.5 4B** (2.7 GB) | 29 s | All 3 decisions, 4 of 5 action items with owners and deadlines exactly as said, 2 of 3 open questions. **Recommended.** |
| **Qwen3.5 9B** (5.7 GB) | 40 s | All 3 decisions, all 5 action items exact; thinner recap. **Larger.** |
| Phi-4-mini (2.5 GB) | 23 s | Invented dates ("2026-09-30" for "end of month"), missed an action item. Out. |
| Gemma 4 E4B (5.0 GB) | — | Its chat template needs llama.cpp's Jinja path, which llama-cpp-2 0.1.159 doesn't expose. Out for now. |

Checks:

- **Wi-Fi off:** the app run inside a sandbox profile that denies all IP traffic (verified: curl to Hugging Face fails in the same profile) took the Zoom call from `transcribing` to a saved local summary and new title in ~50 s.
- **2-hour meeting:** 122 minutes of transcript summarized locally in 1 min 55 s (2 parts + merge); 13 repeats of the same meeting merged back to 4 decisions, 4 action items, 2 questions.
- **BYOK:** no key on this Mac, so the HTTP path is verified against a fake OpenAI-compatible server (request shape, auth, parse); Claude / OpenAI / Gemini request and reply shapes are unit-tested. A real key run is left for Miguel.
- **Malformed output:** fences/prose stripped, one retry with the reason, then transcript-only with "Generate summary" and the reason shown (tests).
- **Real call (Zoom 2:31 PM, same-room test audio):** summary in ~16 s, title "App recording test call". One decision came out backwards ("10 minutes confirmed" when the call said it isn't needed); the noisy two-devices-in-one-room transcript is the hard case for a 4B model.

Decisions and findings:

- **Local AI runs in its own process** (`talaan-llm`, bundled via `externalBin`, signed with the app). Whisper and llama.cpp each bring ggml; in one binary only one copy survives (Parlato hit this and is stuck on llama-cpp-2 0.1.143, which can't run Qwen 3.5 / Gemma 4). Separate processes let Talaan use llama-cpp-2 0.1.159 and the newest models; a crash or out-of-memory in the model can't take the app down.
- `sample()` in llama-cpp-2 already accepts the token; accepting again breaks grammar sampling (`GGML_ASSERT(!stacks.empty())`).
- The first start of a new helper binary can take >15 s while macOS checks it; the start timeout is 60 s.
- Cloud model names aren't hard-coded: the key test lists the provider's models and picks the newest fast tier (Claude Sonnet, GPT-*-mini, Gemini *-flash).
- Keys live in the Keychain (service "Talaan"); `settings.json` has only `byokKeySet`. Delete all data removes them.
- "Summary ready" notifications ask permission on first use, open the meeting on click, and are skipped outside a .app bundle (the notification center throws there).

Left for Miguel at the checkpoint: a real meeting summary in the app, the Summary AI tab (download, Ollama/LM Studio if installed, a real API key), and allowing notifications.
