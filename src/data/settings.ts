import type { ThemePreference } from "../lib/theme";

export type { ThemePreference };
export type CallRule = "ask" | "always" | "never";
export type RetentionChoice = "1d" | "7d" | "30d" | "forever" | "none";
export type TranscriptRetention = "forever" | "1y" | "90d";
export type SummaryEngineKind = "local" | "ollama" | "byok";
export type ByokProvider = "claude" | "openai" | "gemini" | "compatible";
export type BrowserState = "allowed" | "not_asked" | "not_installed";

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
  engine: "whisper" | "parakeet" | "nemotron";
  sizeMb: number;
  speed: 1 | 2 | 3;
  accuracy: 1 | 2 | 3;
  languages: string; // "English only" | "25 European languages" ...
  state: "not_downloaded" | "downloading" | "downloaded" | "on_this_mac";
  progress?: number; // 0..1 while downloading
}

export interface Settings {
  // General
  theme: ThemePreference;
  detectCalls: boolean;
  launchAtLogin: boolean;
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
  reuseModels: boolean;
  meetingLanguage: "auto" | string;
  useGpu: boolean;
  // Summary AI
  summaryEngine: SummaryEngineKind;
  summaryLanguage: "meeting" | "en";
  localModel: "recommended" | "larger";
  ollamaAddress: string;
  ollamaModel?: string;
  byokProvider: ByokProvider;
  byokKeySet: boolean;
  byokModel: string;
  // Connections
  calendarConnected: boolean;
  tabDetection: boolean;
  browsers: { name: string; state: BrowserState }[];
  suggestSpeakerNames: boolean;
  notion: { connected: boolean; workspace?: string; destination?: string };
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
  /** Checks an API key with the provider. The key itself is never stored in settings. */
  testApiKey(provider: ByokProvider, key: string): Promise<{ ok: boolean; message: string }>;
  /** Where the data lives and how much space it uses. */
  dataInfo(): Promise<{ path: string; bytes: number }>;
  /** "Delete all Talaan data": meetings, audio, models, settings and keys. */
  deleteAllData(): Promise<void>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}
