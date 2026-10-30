import type { ThemePreference } from "../lib/theme";

export type { ThemePreference };
export type CallRule = "ask" | "always" | "never";
export type RetentionChoice = "1d" | "7d" | "30d" | "forever" | "none";
export type TranscriptRetention = "forever" | "1y" | "90d";
export type SummaryEngineKind = "local" | "ollama" | "byok";
export type ByokProvider = "claude" | "openai" | "gemini" | "groq" | "cerebras" | "mistral" | "openrouter" | "compatible";
export interface NotionDestination {
  id: string;
  kind: "page" | "database";
  title: string;
}

export type CalendarAccess = "allowed" | "not_asked" | "denied";

/** The meeting the menu bar offers to record (in progress, or starting within the hour), from Rust's `calendar_next`. */
export interface NextMeeting {
  title: string;
  startMs: number;
  endMs: number;
  attendees: string[];
  emails: string[];
  joinUrl?: string | null;
  /** The call app the invite's link opens: "zoom", "teams" or "browser" (Google Meet), and its name. */
  appId?: string | null;
  appLabel?: string | null;
}

/** "Also save each meeting to a folder", for Check setup. */
export interface AutoSaveStatus {
  /** Whether the folder is there right now (a drive may be disconnected). */
  reachable: boolean;
  /** Copies still to be written: `waiting` (asked for), `missing` (the folder wasn't there) or `failed` (with the error). */
  pending: { id: string; kind: "waiting" | "missing" | "failed"; error?: string }[];
}

/** What saving a copy did: see `autosave_write` in commands/autosave.rs. */
export type AutoSaveResult = "written" | "off" | "not_ready" | "missing" | "failed";

/** Check setup › Graphics chip, from Rust (`commands/gpu.rs`). */
export interface GpuStatus {
  /** `metal`: the Mac's GPU. `unavailable`: see `reason`. `failed`: it failed with this app and driver (Try again). */
  state: "metal" | "in_use" | "off" | "unavailable" | "failed";
  reason?: "no_gpu";
  /** The GPU in use ("NVIDIA GeForce RTX 5070 Ti"). */
  device?: string;
  vramMb?: number;
  /** For `failed`: codes for the jobs that used the processor, "stt" (speech), "llm" (AI) or "stt,llm". */
  detail?: string;
}
export type BrowserState = "allowed" | "denied" | "not_asked" | "not_installed";

export interface CallAppRule {
  id: string;
  name: string;
  note: string;
  rule: CallRule;
  /** Browser calls: Always needs browser tab detection turned on. */
  isBrowser?: boolean;
}

export interface IgnoredApp {
  name: string;
  why: string;
}

export interface SpeechModel {
  id: string;
  name: string;
  maker: string;
  note: string;
  engine: "whisper" | "parakeet" | "diarization" | "search";
  sizeMb: number;
  speed: 1 | 2 | 3;
  accuracy: 1 | 2 | 3;
  languages: string; // "English" | "25 European languages" ...
  /** SPDX licence id of the model ("MIT", "CC-BY-4.0"). */
  licence: string;
  /** Listens in English only: other languages come out wrong. */
  englishOnly: boolean;
  state: "not_downloaded" | "downloading" | "downloaded";
  progress?: number; // 0..1 while downloading
  /** What an interrupted download left on disk (0..1), while not downloading; Download resumes from it. */
  partial?: number;
  /** Why the last download failed, until the next try. */
  error?: string;
  /** The partial download was stopped with Cancel: it waits for a click (never resumed at launch). */
  cancelled?: boolean;
}

/** A local summary AI model (downloaded once, runs on this computer). */
export interface LocalModel {
  id: string;
  name: string;
  note: string;
  sizeMb: number;
  /** The one Talaan suggests first. */
  recommended: boolean;
  /** SPDX licence id. */
  licence: string;
  /** Rough memory needed to run it, in GB. */
  memoryGb: number;
  state: "not_downloaded" | "downloading" | "downloaded";
  progress?: number;
  /** What an interrupted download left on disk (0..1), while not downloading; Download resumes from it. */
  partial?: number;
  /** Why the last download failed, until the next try. */
  error?: string;
  /** The partial download was stopped with Cancel: it waits for a click (never resumed at launch). */
  cancelled?: boolean;
}

export interface Settings {
  // General
  theme: ThemePreference;
  /** What the transcript, summaries and exports call the user (instead of "Me"). Empty = "Me". */
  yourName: string;
  detectCalls: boolean;
  /** Tag a detected call's meeting with the app's name. */
  autoTagApps: boolean;
  launchAtLogin: boolean;
  /** Look for a new version on GitHub at launch (only the version is fetched; nothing is sent). */
  checkUpdates: boolean;
  recordingNotice: string;
  microphone: string; // "default" or a device name
  warnOnSpeakers: boolean;
  /** The sound enhancer whose notice the recording popover already showed (Windows; shown once per enhancer). */
  soundRelayNoticeSeen?: string;
  notifySummaryReady: boolean;
  // Call apps
  callApps: CallAppRule[];
  ignoredApps: IgnoredApp[];
  // Speech models
  speechModels: SpeechModel[];
  activeSpeechModel: string;
  /** The speaker separation model (from disk in the app, like speechModels). */
  speakerModel: SpeechModel;
  /** Split the other side into Speaker 1, 2, 3 when the speaker model is downloaded. */
  separateSpeakers: boolean;
  /** Ask's search by meaning (from disk in the app). */
  searchModel: SpeechModel;
  meetingLanguage: "auto" | string;
  /** The language of Talaan's own screens: "auto" follows the system. Not the language of meetings or summaries. */
  uiLanguage: "auto" | "en" | "fr" | "es";
  useGpu: boolean;
  // Summary AI
  summaryEngine: SummaryEngineKind;
  /** Summarize each new transcript by itself. Off: meetings wait for Generate summary. */
  summarizeAutomatically: boolean;
  /** "meeting" = the meeting's own language, or an ISO code. */
  summaryLanguage: string;
  /** The meeting type new recordings are summarized as (lib/templates). */
  summaryTemplate: string;
  /** Applied to every summary ("Always mention budget numbers"). */
  summaryInstructions: string;
  /** The id of the local summary model in use (one of localModels). */
  localModel: string;
  /** From disk in the app (like speechModels); never stored in settings.json. */
  localModels: LocalModel[];
  ollamaAddress: string;
  ollamaModel?: string;
  byokProvider: ByokProvider;
  /** Providers with a key in secure key storage (the keys themselves never come here). */
  byokKeys: ByokProvider[];
  byokModel: string;
  /** Base URL for "Other (OpenAI-compatible)". */
  byokBaseUrl?: string;
  // Connections
  calendarConnected: boolean;
  tabDetection: boolean;
  suggestSpeakerNames: boolean;
  /** The secret itself lives in secure key storage, never here. */
  notion: { connected: boolean; workspace?: string; destination?: NotionDestination };
  /** "Also save each meeting to a folder": where (empty = off). */
  autoSaveFolder: string;
  /** Put the transcript in those copies too (meetings without a summary always have it). */
  autoSaveTranscript: boolean;
  // Privacy
  retention: RetentionChoice;
  transcriptRetention: TranscriptRetention;
  // App state
  onboardingDone: boolean;
  /** Windows: the one-time "Talaan lives in the system tray" hint after onboarding. Set when onboarding ends. */
  showTrayTip?: boolean;
  /** The guided test recording's meeting, until the user deletes or keeps it. */
  testMeetingId?: string;
}

export interface SettingsStore {
  get(): Promise<Settings>;
  update(patch: Partial<Settings>): Promise<Settings>;
  /** Starts (or simulates) a model download; progress arrives through `subscribe`. */
  downloadModel(id: string): Promise<void>;
  /** Stops a download. The part already downloaded is kept, so Download resumes. */
  cancelModelDownload(id: string): Promise<void>;
  /** Stops Talaan using a model. The file is erased unless another app (`otherApps`) still uses it. */
  deleteModel(id: string): Promise<{ erased: boolean; otherApps: string[] }>;
  /** Checks an API key with the provider and, if it works, saves it in the Keychain (never in settings). */
  testApiKey(provider: ByokProvider, key: string): Promise<{ ok: boolean; message: string; model?: string }>;
  /** Removes a saved key from the Keychain. */
  clearApiKey(provider: ByokProvider): Promise<void>;
  /** Removes every provider's saved key (Privacy › Saved API keys). */
  clearAllApiKeys(): Promise<void>;
  /** Models a local server (Ollama, LM Studio) offers at an address. Throws when it can't be reached. */
  localServerModels(address: string): Promise<string[]>;
  /** Checks an integration secret with Notion and saves it; returns the workspace name. */
  connectNotion(secret: string): Promise<string>;
  disconnectNotion(): Promise<void>;
  /** Pages and databases shared with the integration. */
  notionDestinations(): Promise<NotionDestination[]>;
  /** Browsers tab detection works with, and whether Talaan may read their tabs. */
  browsersStatus(): Promise<{ name: string; state: BrowserState }[]>;
  /** Connected microphones by name. */
  audioInputs(): Promise<string[]>;
  /** A newer version on GitHub, if any. */
  checkForUpdate(): Promise<{ version: string; notes?: string } | null>;
  /** The newer version the last check found (at launch, every 6 hours, or Check now), if any. */
  pendingUpdate(): Promise<{ version: string; notes?: string | null } | null>;
  /** Downloads, installs and relaunches. */
  installUpdate(): Promise<void>;
  /** Calendar access as the system reports it. */
  calendarAccess(): Promise<CalendarAccess>;
  /** Asks the system for calendar access (the prompt shows only the first time). */
  requestCalendar(): Promise<CalendarAccess>;
  /**
   * Windows: downloads the calendar's private iCal address and, if it works, saves it in secure key storage (never
   * in settings) and turns the calendar on. Returns the events in the next 7 days. Throws a plain message.
   */
  connectCalendar(url: string): Promise<number>;
  /** Windows: forgets the calendar address and turns the calendar off. */
  disconnectCalendar(): Promise<void>;
  /** The meeting coming up (or in progress) when the calendar is connected; null otherwise. */
  nextMeeting(): Promise<NextMeeting | null>;
  /** The folder picker. Undefined when cancelled. */
  pickFolder(title: string): Promise<string | undefined>;
  /** Writes a meeting's copy in the auto-save folder (`contents`: the Markdown export). */
  writeAutoSave(id: string, contents: string, finished: boolean): Promise<AutoSaveResult>;
  /** Null when no folder is chosen. */
  autoSaveStatus(): Promise<AutoSaveStatus | null>;
  /** Shows the auto-save folder in the file manager. */
  revealAutoSaveFolder(): Promise<void>;
  /** Whether the GPU is used for speech and local summaries, and if not, why. */
  gpuStatus(): Promise<GpuStatus>;
  /** Forgets a remembered GPU failure and checks again. */
  retryGpu(): Promise<GpuStatus>;
  /** Where the data lives and how much space it uses. */
  dataInfo(): Promise<{ path: string; bytes: number }>;
  /** Shows the data folder in the file manager. */
  revealData(): Promise<void>;
  /** "Delete all Talaan data": meetings, audio, models, settings and keys. */
  deleteAllData(): Promise<void>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}

/** Whether the chosen provider has a saved key. */
export function hasByokKey(s: Pick<Settings, "byokProvider" | "byokKeys">): boolean {
  return s.byokKeys.includes(s.byokProvider);
}
