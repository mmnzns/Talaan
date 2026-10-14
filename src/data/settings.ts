import type { ThemePreference } from "../lib/theme";

export type { ThemePreference };
export type CallRule = "ask" | "always" | "never";
export type RetentionChoice = "1d" | "7d" | "30d" | "forever" | "none";
export type SummaryEngineKind = "local" | "ollama" | "lmstudio" | "byok";
export type ByokProvider = "claude" | "openai" | "gemini" | "compatible";

export interface CallAppRule {
  id: string;
  name: string;
  rule: CallRule;
  /** Only browser calls: Always needs tab detection turned on. */
  isBrowser?: boolean;
}

export interface SpeechModel {
  id: string;
  name: string;
  engine: "whisper" | "parakeet" | "nemotron";
  sizeMb: number;
  languages: string; // "English only" | "99 languages" ...
  state: "not_downloaded" | "downloading" | "downloaded" | "on_this_mac";
  progress?: number; // 0..1 while downloading
  recommended?: boolean;
}

export interface Settings {
  theme: ThemePreference;
  summaryLanguage: "meeting" | string; // "meeting" = write in the meeting's language
  launchAtLogin: boolean;
  callApps: CallAppRule[];
  ignoredApps: string[];
  tabDetection: boolean;
  speechModels: SpeechModel[];
  activeSpeechModel: string;
  summaryEngine: SummaryEngineKind;
  localModel: string;
  byokProvider: ByokProvider;
  byokKeySet: boolean;
  calendarConnected: boolean;
  notion: { connected: boolean; workspace?: string };
  retention: RetentionChoice;
  onboardingDone: boolean;
}

export interface SettingsStore {
  get(): Promise<Settings>;
  update(patch: Partial<Settings>): Promise<Settings>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}
