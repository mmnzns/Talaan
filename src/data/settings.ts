import type { ThemePreference } from "../lib/theme";

export type { ThemePreference };
export type CallRule = "ask" | "always" | "never";
export type RetentionChoice = "1d" | "7d" | "30d" | "forever" | "none";
export type TranscriptRetention = "forever" | "1y" | "90d";
export type SummaryEngineKind = "local" | "ollama" | "byok";
export type ByokProvider = "claude" | "openai" | "gemini" | "compatible";
export interface NotionDestination {
  id: string;
  kind: "page" | "database";
  title: string;
}

export type CalendarAccess = "allowed" | "not_asked" | "denied";
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
  languages: string; // "English only" | "25 European languages" ...
  state: "not_downloaded" | "downloading" | "downloaded" | "on_this_mac";
  progress?: number; // 0..1 while downloading
}

/** A local summary AI model (downloaded once, runs on this computer). */
export interface LocalModel {
  id: string;
  choice: "recommended" | "larger";
  name: string;
  note: string;
  sizeMb: number;
  state: "not_downloaded" | "downloading" | "downloaded";
  progress?: number;
}

export interface Settings {
  // General
  theme: ThemePreference;
  detectCalls: boolean;
  /** Tag a detected call's meeting with the app's name. */
  autoTagApps: boolean;
  launchAtLogin: boolean;
  /** Look for a new version on GitHub at launch (only the version is fetched; nothing is sent). */
  checkUpdates: boolean;
  recordingNotice: string;
  microphone: string; // "default" or a device name
  warnOnSpeakers: boolean;
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
  reuseModels: boolean;
  meetingLanguage: "auto" | string;
  /** The language of Talaan's own screens: "auto" follows the system. Not the language of meetings or summaries. */
  uiLanguage: "auto" | "en" | "fr" | "es";
  useGpu: boolean;
  // Summary AI
  summaryEngine: SummaryEngineKind;
  /** "meeting" = the meeting's own language, or an ISO code. */
  summaryLanguage: string;
  localModel: "recommended" | "larger";
  /** From disk in the app (like speechModels); never stored in settings.json. */
  localModels: LocalModel[];
  ollamaAddress: string;
  ollamaModel?: string;
  byokProvider: ByokProvider;
  byokKeySet: boolean;
  byokModel: string;
  /** Base URL for "Other (OpenAI-compatible)". */
  byokBaseUrl?: string;
  // Connections
  calendarConnected: boolean;
  tabDetection: boolean;
  suggestSpeakerNames: boolean;
  /** The secret itself lives in secure key storage, never here. */
  notion: { connected: boolean; workspace?: string; destination?: NotionDestination };
  // Privacy
  retention: RetentionChoice;
  transcriptRetention: TranscriptRetention;
  // App state
  onboardingDone: boolean;
}

export interface SettingsStore {
  get(): Promise<Settings>;
  update(patch: Partial<Settings>): Promise<Settings>;
  /** Starts (or simulates) a model download; progress arrives through `subscribe`. */
  downloadModel(id: string): Promise<void>;
  /** Stops a download. The part already downloaded is kept, so Download resumes. */
  cancelModelDownload(id: string): Promise<void>;
  /** Deletes Talaan's own copy of a model (models reused from another app are never touched). */
  deleteModel(id: string): Promise<void>;
  /** Checks an API key with the provider and, if it works, saves it in the Keychain (never in settings). */
  testApiKey(provider: ByokProvider, key: string): Promise<{ ok: boolean; message: string; model?: string }>;
  /** Removes a saved key from the Keychain. */
  clearApiKey(provider: ByokProvider): Promise<void>;
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
  /** Downloads, installs and relaunches. */
  installUpdate(): Promise<void>;
  /** Calendar access as the system reports it. */
  calendarAccess(): Promise<CalendarAccess>;
  /** Asks the system for calendar access (the prompt shows only the first time). */
  requestCalendar(): Promise<CalendarAccess>;
  /** Where the data lives and how much space it uses. */
  dataInfo(): Promise<{ path: string; bytes: number }>;
  /** "Delete all Talaan data": meetings, audio, models, settings and keys. */
  deleteAllData(): Promise<void>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}
