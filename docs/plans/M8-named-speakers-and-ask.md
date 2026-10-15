# M8. Named speakers and Ask — task plan

> Generated from Notion "Talaan v1 Build Plan" › M8. Model: **Opus**. 🛑 checkpoint skipped (Miguel tests the whole app at the end, see `docs/TESTING.md`).
> Done when: a 3-person call separates into 3 speakers with reasonable accuracy; renaming updates the transcript and summary owners; a question across 5+ meetings answers with correct citations, Wi-Fi off.

## Verified facts (2026-10-14)

- **Speaker model:** NVIDIA Nemotron 3 Diarization (streaming Sortformer, up to 8 speakers), license OpenMDW-1.1, ONNX export `nemotron3_diar_v3.onnx` (400,506,656 bytes, sha256 `915e4f…be03a`) in `altunenes/parakeet-rs` at a pinned revision. Runs through parakeet-rs 0.3.8's `sortformer` feature (already a dependency). Sortformer v2 exports are rejected by 0.3.8.
- **Streaming warm-up:** the model only tells a voice apart after hearing it. Feeding the first 10 minutes once (predictions thrown away) before the real pass took a 3-voice test from 8/12 to 12/12 turns right.
- **Search model:** Qwen3-Embedding 0.6B, official GGUF `Qwen3-Embedding-0.6B-Q8_0.gguf` (639,150,592 bytes, sha256 `06507c…c439`), Apache-2.0, 100+ languages, 1024 dims, last-token pooling from its metadata. Runs in the existing `talaan-llm` helper (llama.cpp) via a new `embed` request.

## Architecture decisions (Opus, binding)

### Named speakers
- `transcriber/diarize.rs`: `SpeakerSplitter` trait (`activity(them) -> frames × speakers`), `Diarizer` (warm-up, 5-minute chunks), `assign_speakers` (each Them sentence takes its dominant speaker over its span; speakers numbered by first appearance; unclear sentences follow the previous speaker; fewer than two voices stay plain "Them").
- Pipeline runs it after echo removal, before merge; merge never joins different speakers. The worker loads the model lazily when downloaded and `separateSpeakers` is on (default on); a model that fails to load costs only the split, never the transcript. `people_count` = speakers + 1.
- Renaming a speaker also moves action items whose owner was the old display name (both the original and edited summary).
- The model is an add-on row in Settings › Speech models (download / cancel / delete / on-off), served by the same model commands.

### Ask
- `ask/passages.rs`: transcript windows (≤ 700 chars or 90 s, one line of overlap, "Name: text" lines with their start times) plus summary items (recap, decisions, action items with owner/due, open questions) per meeting.
- `ask/search.rs`: BM25 over terms (stopwords dropped) always; cosine over embeddings when the search model is downloaded; reciprocal rank fusion (k = 60).
- `ask/index.rs`: vectors in `search.bin` beside each meeting, fingerprinted by model id + passage text, so edits and new summaries rebuild it. Built on demand at question time and by a debounced background indexer (after `meetings-changed`, at launch, after the search model downloads).
- `ask/answer.rs`: one fixed prompt and JSON schema for every engine: `answer` + up to 6 `points`, each naming its excerpt number. Citations land on the line inside the cited window that best matches the point. Unknown sources keep the point without a citation; an empty answer is "couldn't find it".
- `commands/ask.rs`: `meetings_ask(question, scope)` with scope meeting / all / folder / tag; uses the user's summary engine (local, local server, or own key); a single meeting that fits the context is sent whole, otherwise the top 14 passages within the context budget. One retry on an unreadable reply.

## Tasks
1. Diarization module + tests (numbering, single voice, unclear sentences, Me lines untouched) + real-model smoke test.
2. Pipeline and worker wiring, setting, people count, catalog entry, Settings row, frontend naming (Speaker N everywhere).
3. Rename moves action items.
4. Helper `embed` request + client.
5. Passages, search, index, answer modules (TDD).
6. `meetings_ask`, background indexer, search model in catalog and Settings, frontend store + error messages.
7. Real checks: 3-voice pipeline, embedding ranking, 6-meeting Ask with network denied; docs, Build Log.

## Out of scope
Calendar attendee name suggestions for speakers (M9 with EventKit), streaming the answer as it's written.

## Outcome (2026-10-14)

All tasks done. 248 Rust tests (14 ignored real-model smoke tests), 77 TS tests.

- **Named speakers:** the Karen / Daniel / Rishi three-voice call through the full pipeline (Whisper Turbo + Nemotron): 12/12 lines on the right speaker, 3 speakers found, 6.9 s for the minute of audio. Two of macOS's TTS voices (Samantha, Rishi) are indistinguishable to the model; real voices differ far more.
- **Search model:** "Who talks to the lawyers?" finds "chase legal for the copy" (no shared word); an English question finds a Filipino passage.
- **Ask:** six meetings, four questions (vendor SSO promise, who is emailing legal, the pricing decision, the overspend fix): every answer correct and citing the exact line, 4 to 7 s each with the local Qwen3.5 4B; the same run passed with all networking denied by `sandbox-exec`.

Not verified with real meetings (needs Miguel): diarization on a real multi-person call, Ask on his own library, the Settings rows and Ask screen in the app.
