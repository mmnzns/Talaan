# M1. Full UI with sample data — task plan

> Generated from Notion "Talaan v1 Build Plan" › M1. Model: **Sonnet** (plan written by Opus).
> Ends at a 🛑 checkpoint: Miguel clicks through every screen in light and dark before any native work.

## Goal

Every UI v2 screen built in React with sample data, in Talaan's design language, light and dark. All data flows through one `MeetingStore` interface so M2 swaps in real storage without touching screens.

## Source of truth for the look

`Google Drive › MAIN DRIVE - Miguel › FILES - Projects & Work › Talaan - Meeting Minutes › Design/UI v2/`
(absolute: `/Users/miguel.monzones/My Drive (mikeymonzones@gmail.com)/MAIN DRIVE - Miguel/FILES - Projects & Work/Talaan - Meeting Minutes/Design/UI v2/`).

Each `*.dc.html` is a design-canvas source: inline styles plus a small `DCLogic` class for interactive state. Read them for layout, spacing, copy and states. **Rebuild them as Talaan components on the tokens in `src/styles/tokens.css`**: no hex values in components (use the Tailwind token classes such as `bg-paper`, `text-subtle`, `shadow-hard`), no fake traffic lights or fake menu bar (those are canvas chrome). Design empty and error states yourself in the same style. Copy text from the designs is approved; keep it.

Rules: plain cream background, no pattern. Don't copy Parlato component files (build rule 6).

## Architecture decisions (Opus, binding)

### Windows

| Surface | M1 implementation | Becomes real in |
|---|---|---|
| Main window (split view, Ask, Export sheet, Edit mode) | Tauri `main` window | — |
| Settings (6 tabs) | Second Tauri window, label `settings`, opened from the main window's toolbar and `Cmd+,` | — |
| Onboarding (7 steps) | Rendered inside `main` when `store.getOnboardingDone()` is false; dev toggle to re-show | M10 |
| Menu bar popover idle / recording, Nudge, Banner, Call ended | Standalone components shown on a dev-only **Screens** stage in the main window (`#/screens`), each at its real size | M6 (mounted in real tray/overlay windows) |

Use `titleBarStyle: "Overlay"` + `hiddenTitle: true` on `main` so the toolbar sits in the title bar area like the design; leave ~78px left padding for the native traffic lights and mark the toolbar `data-tauri-drag-region`.

Routing: a tiny hash router (`#/`, `#/meeting/:id`, `#/ask`, `#/screens`, settings window uses `#/settings/:tab`). No router dependency.

### Data layer (the contract M2 to M9 build on)

`src/data/types.ts`:

```ts
export type MeetingId = string;
export type Ms = number;                      // milliseconds from recording start
export type Speaker = "me" | `them:${number}`; // them:0 = unnamed "Them"; them:1.. = diarized speakers

export type MeetingStatus = "recording" | "transcribing" | "summarizing" | "ready" | "failed" | "needs_recovery";
export type CallApp = "zoom" | "meet" | "teams" | "slack" | "facetime" | "webex" | "browser" | "import" | "other";

export interface MeetingSummaryItem { id: string; text: string; at?: Ms; }
export interface ActionItem extends MeetingSummaryItem { owner?: string; due?: string; done: boolean; }

export interface Summary {
  recap: string;
  decisions: MeetingSummaryItem[];
  actionItems: ActionItem[];
  openQuestions: MeetingSummaryItem[];
  engine: string;          // "Local AI · <model>" | "Claude" | ...
  edited: boolean;         // true when the user changed it (original kept in storage)
}

export interface TranscriptLine { id: string; at: Ms; end: Ms; speaker: Speaker; text: string; edited: boolean; }
export interface Mark { id: string; at: Ms; note?: string; }
export interface PauseGap { at: Ms; durationMs: Ms; }

export interface MeetingListItem {
  id: MeetingId; title: string; app: CallApp; appLabel: string;
  startedAt: string;       // ISO
  durationMs: Ms; status: MeetingStatus; progress?: number; // 0..1 while transcribing/summarizing
  folderId?: string; tags: string[]; peopleCount: number;
}

export interface Meeting extends MeetingListItem {
  language: string;                     // BCP-47, e.g. "en"
  audioKeptUntil?: string;              // ISO; undefined = audio deleted or not kept
  pinned: boolean;
  calendarEvent?: { title: string; attendees: string[] };
  speakers: Partial<Record<Speaker, string>>; // display names: { me: "Me", "them:1": "Dana Reyes" }
  summary?: Summary;                    // undefined = transcript-only ("Generate summary")
  transcript: TranscriptLine[];
  marks: Mark[];
  pauses: PauseGap[];
}

export interface Folder { id: string; name: string; count: number; }

export interface AskCitation { meetingId: MeetingId; meetingTitle: string; date: string; at: Ms; }
export interface AskAnswer { text: string; points: { text: string; citation?: AskCitation }[]; fromMeetings: number; totalMeetings: number; }
export type AskScope = { kind: "meeting"; id: MeetingId } | { kind: "all" } | { kind: "folder"; id: string } | { kind: "tag"; tag: string };
```

`src/data/store.ts` — the interface. Async everywhere (Tauri commands are async), so the mock must be too:

```ts
export interface MeetingStore {
  listMeetings(filter?: { folderId?: string; tag?: string; query?: string }): Promise<MeetingListItem[]>;
  getMeeting(id: MeetingId): Promise<Meeting | undefined>;
  renameMeeting(id: MeetingId, title: string): Promise<void>;
  setPinned(id: MeetingId, pinned: boolean): Promise<void>;
  deleteMeeting(id: MeetingId): Promise<void>;
  updateTranscriptLine(id: MeetingId, lineId: string, text: string): Promise<void>;
  updateSummary(id: MeetingId, summary: Summary): Promise<void>;
  revertEdits(id: MeetingId): Promise<void>;
  renameSpeaker(id: MeetingId, speaker: Speaker, name: string): Promise<void>;
  setTags(id: MeetingId, tags: string[]): Promise<void>;
  moveToFolder(id: MeetingId, folderId?: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  listTags(): Promise<string[]>;
  ask(question: string, scope: AskScope): Promise<AskAnswer>;
  subscribe(listener: () => void): () => void;   // fires on any change
}
```

Settings get their own small interface in `src/data/settings.ts` (`SettingsStore` with `get(): Promise<Settings>`, `update(patch)`, `subscribe`), typed from the Settings window fields (theme, summary language, call app rules, ignored apps, speech model, summary engine, connections, retention). Mock with in-memory state.

`src/data/mock/` holds sample data matching the design (Q4 roadmap sync with Dana Reyes and Priya Shah, Vendor demo in "summarizing", Weekly 1:1, Website launch retro, Imported client kickoff.m4a, folders Work/Clients/Personal, tags #roadmap #vendors #1:1 #zoom) plus at least one transcript-only meeting, one `failed`, one `needs_recovery`. `src/data/index.ts` exports a single `store` instance; screens get it via a `StoreProvider` React context and a `useStore` hook, never by importing the mock directly.

### Components

`src/ui/` is Talaan's own kit, built from the design's recurring pieces: `Button` (default / primary-lime / danger, hard shadow), `Chip`, `Timestamp` (clickable mono pill), `Label` (mono caption), `Switch`, `Checkbox`, `Radio`, `Field`, `Select`, `Sheet` (modal with hard shadow), `SectionHeader`. Screens live in `src/screens/<Screen>/`.

## Tasks

Commit after each task once `npm test` and `npm run build` pass. Message prefix `M1:`.

1. **Data contract.** `types.ts`, `store.ts`, `settings.ts`, mock data, `StoreProvider`/`useStore`. Tests (Vitest): mock filters by folder, tag and query; rename / pin / speaker rename / edit + revert change what `getMeeting` returns; `subscribe` fires on change.
2. **UI kit** in `src/ui/` with a light/dark check on the Screens stage.
3. **App shell + router + window setup.** Hash router, overlay title bar, toolbar (Offline · Local AI status, Ask, Import audio, Record), Settings window (`tauri.conf.json` second window hidden by default or created via `WebviewWindow`; add the capability permissions it needs).
4. **Main window left column:** search, folders with counts, tags, meetings grouped by day (today / yesterday / dates), status badges with progress, empty state (no meetings yet), no-results state.
5. **Meeting view:** header (title editable, app, date, duration, language, audio-kept-until, folder, tags + add tag, calendar badge with attendees), summary column (recap, decisions, action items with owner/due/time + checkbox, open questions), transcript column (speakers panel with rename, lines with speaker names, marks highlighted, pause gaps "Paused 3 min", now/total time), clicking any timestamp scrolls to and highlights the transcript line. Transcript-only state with "Generate summary". Failed and needs-recovery states (Recover / Discard).
6. **Edit mode:** edit transcript lines and summary text, "Edited" indicator, Revert to original.
7. **Copy for Slack / email** (format the summary to the clipboard) and **Export sheet** (Markdown, PDF, Plain text, Notion with destination picker, include-transcript switch). Buttons show the result state; no real file writing yet.
8. **Ask screen:** scope chips (This meeting / All meetings / Folder / Tag), answer with citation pills that open the meeting at that time, suggested questions, input.
9. **Settings window:** General (theme System/Light/Dark wired to `applyTheme`, summary language, launch at login), Call apps (per-app Ask/Always/Never; Browser calls disabled Always unless tab detection on; ignored apps list add/remove), Speech models (list, download progress, "On this Mac", languages), Summary AI (Local AI default with model, Ollama/LM Studio, your own key with Test), Connections (Calendar, Notion, browser tab detection), Privacy (retention 1/7/30/forever/don't keep, What leaves this Mac, Delete all Talaan data with confirm).
10. **Onboarding** (7 steps: welcome, microphone, system audio, speech model, summary engine, call apps, done) with Back/Next and step indicator.
11. **Recording surfaces** on the `#/screens` stage: menu bar popover idle, recording (timer, levels Me/Them, Copy notice, speakers warning, Pause/Resume, Mark, quick note, Stop and save, Discard), Nudge, Always banner, Call ended (30 s countdown). Self-contained components with props, no store coupling beyond callbacks.
12. **Dark mode pass:** every screen in both themes, theme override persists via `SettingsStore` (mock: `localStorage`). Fix contrast issues.
13. **Checkpoint prep:** README section "Click through M1", screenshots of every screen light + dark in `docs/screens/M1/`, update CLAUDE.md file map, add a Build Log entry to Notion (Talaan Journey › Build Log). Then **stop for Miguel's review.**

## Done checks

- `npm test`, `npm run build`, `cargo test` pass; CI green.
- Every screen reachable by clicking (Screens stage for the M6 surfaces), in light and dark.
- No hex colors in `src/screens` or `src/ui` (`grep -rn "#[0-9A-Fa-f]\{6\}" src/screens src/ui` returns nothing).
- No screen imports from `src/data/mock` directly.

## Outcome (2026-10-14)

All 13 tasks done. The contract grew in these additive ways while building the screens:

- `MeetingStore`: `createFolder`, `recoverMeeting`, `retryProcessing`, `generateSummary`.
- `MeetingListItem` carries `pinned` (so the list can show the pin without loading each meeting).
- `SettingsStore`: `downloadModel`, `testApiKey`, `deleteAllData`.
- `Settings` expanded to match the Settings and Onboarding designs (recording notice, microphone, per-model details, Ollama address, transcript retention, browser states, Notion default destination, ...).
- Mock-only helpers: `?mock=empty` URL flag, simulated progress for recover / retry / summary / model download.

Not built (by design): real audio, real files, real native windows for the menu bar surfaces. Those arrive with M2 and M6.
