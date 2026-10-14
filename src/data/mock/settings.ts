import type { Settings, SettingsStore } from "../settings";

const KEY = "talaan.mock.settings";

export const defaultSettings: Settings = {
  theme: "system",
  summaryLanguage: "meeting",
  launchAtLogin: true,
  callApps: [
    { id: "zoom", name: "Zoom", rule: "always" },
    { id: "slack", name: "Slack", rule: "ask" },
    { id: "teams", name: "Microsoft Teams", rule: "ask" },
    { id: "facetime", name: "FaceTime", rule: "ask" },
    { id: "webex", name: "Webex", rule: "ask" },
    { id: "browser", name: "Browser calls", rule: "ask", isBrowser: true },
  ],
  ignoredApps: [
    "Talaan",
    "Parlato",
    "VoiceInk",
    "Superwhisper",
    "Wispr Flow",
    "MacWhisper",
    "Apple Dictation",
    "Voice Memos",
  ],
  tabDetection: false,
  speechModels: [
    { id: "parakeet-v3", name: "Parakeet v3", engine: "parakeet", sizeMb: 640, languages: "25 European languages", state: "on_this_mac", recommended: true },
    { id: "whisper-large-v3-turbo", name: "Whisper large v3 turbo", engine: "whisper", sizeMb: 1620, languages: "99 languages", state: "not_downloaded" },
    { id: "whisper-small", name: "Whisper small", engine: "whisper", sizeMb: 466, languages: "99 languages", state: "downloaded" },
    { id: "nemotron-streaming", name: "Nemotron streaming", engine: "nemotron", sizeMb: 2400, languages: "English only", state: "not_downloaded" },
  ],
  activeSpeechModel: "parakeet-v3",
  summaryEngine: "local",
  localModel: "Recommended small model",
  byokProvider: "claude",
  byokKeySet: false,
  calendarConnected: false,
  notion: { connected: true, workspace: "Miguel" },
  retention: "7d",
  onboardingDone: true,
};

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...defaultSettings, ...JSON.parse(raw) };
  } catch {
    /* storage unavailable: fall back to defaults */
  }
  return structuredClone(defaultSettings);
}

/** In-memory SettingsStore persisted to localStorage so both windows share it. Replaced in M2/M10. */
export class MockSettingsStore implements SettingsStore {
  private state = load();
  private listeners = new Set<() => void>();

  constructor() {
    if (typeof window !== "undefined") {
      window.addEventListener("storage", (e) => {
        if (e.key === KEY) {
          this.state = load();
          this.emit();
        }
      });
    }
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  async get(): Promise<Settings> {
    return structuredClone(this.state);
  }

  async update(patch: Partial<Settings>): Promise<Settings> {
    this.state = { ...this.state, ...patch };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.state));
    } catch {
      /* ignore */
    }
    this.emit();
    return structuredClone(this.state);
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
