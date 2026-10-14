import type { ByokProvider, Settings, SettingsStore } from "../settings";

const KEY = "talaan.mock.settings";

export const defaultSettings: Settings = {
  theme: "system",
  detectCalls: true,
  launchAtLogin: true,
  recordingNotice: "Heads up: I'm recording this call for my own notes. It stays on my computer.",
  microphone: "default",
  warnOnSpeakers: true,
  notifySummaryReady: true,
  callApps: [
    { id: "zoom", name: "Zoom", note: "zoom.us app", rule: "always" },
    { id: "slack", name: "Slack", note: "Huddles and calls", rule: "ask" },
    { id: "teams", name: "Microsoft Teams", note: "Teams app", rule: "ask" },
    {
      id: "browser",
      name: "Browser calls",
      note: "Google Meet, Zoom web, Teams web. Always needs browser detection (Connections)",
      rule: "ask",
      isBrowser: true,
    },
    { id: "facetime", name: "FaceTime", note: "Personal calls", rule: "never" },
    { id: "webex", name: "Webex", note: "Webex app", rule: "ask" },
  ],
  ignoredApps: [
    { name: "Parlato", why: "dictation" },
    { name: "VoiceInk", why: "dictation" },
    { name: "Superwhisper", why: "dictation" },
    { name: "Wispr Flow", why: "dictation" },
    { name: "MacWhisper", why: "transcription" },
    { name: "Apple Dictation", why: "built in" },
    { name: "Voice Memos", why: "built in" },
  ],
  speechModels: [
    { id: "parakeet", name: "Parakeet", maker: "NVIDIA", note: "Fast and accurate. 25 European languages.", engine: "parakeet", sizeMb: 600, speed: 3, accuracy: 2, languages: "25 European languages", state: "on_this_mac" },
    { id: "nemotron", name: "Nemotron", maker: "NVIDIA", note: "Newest and most accurate on long calls. English.", engine: "nemotron", sizeMb: 2400, speed: 2, accuracy: 3, languages: "English only", state: "downloading", progress: 0.38 },
    { id: "whisper-large", name: "Whisper Large", maker: "OpenAI", note: "Widest language support, including Filipino.", engine: "whisper", sizeMb: 1600, speed: 1, accuracy: 3, languages: "99 languages", state: "on_this_mac" },
    { id: "whisper-small", name: "Whisper Small", maker: "OpenAI", note: "Light and quick on older Macs. Lower accuracy.", engine: "whisper", sizeMb: 500, speed: 3, accuracy: 1, languages: "99 languages", state: "not_downloaded" },
  ],
  activeSpeechModel: "parakeet",
  reuseModels: true,
  meetingLanguage: "auto",
  useGpu: true,
  summaryEngine: "local",
  summaryLanguage: "meeting",
  localModel: "recommended",
  ollamaAddress: "http://localhost:11434",
  byokProvider: "claude",
  byokKeySet: false,
  byokModel: "recommended",
  calendarConnected: true,
  tabDetection: true,
  browsers: [
    { name: "Chrome", state: "allowed" },
    { name: "Arc", state: "allowed" },
    { name: "Safari", state: "not_asked" },
    { name: "Edge", state: "not_installed" },
  ],
  suggestSpeakerNames: true,
  notion: { connected: true, workspace: "Miguel", destination: "Meeting Notes (database)" },
  retention: "7d",
  transcriptRetention: "forever",
  onboardingDone: true,
};

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...structuredClone(defaultSettings), ...JSON.parse(raw) };
  } catch {
    /* storage unavailable: fall back to defaults */
  }
  return structuredClone(defaultSettings);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** In-memory SettingsStore persisted to localStorage so both windows share it. Replaced in M2/M10. */
export class MockSettingsStore implements SettingsStore {
  private state = load();
  private listeners = new Set<() => void>();

  constructor() {
    if (typeof window !== "undefined") {
      window.addEventListener("storage", (e) => {
        if (e.key === KEY || e.key === null) {
          this.state = load();
          this.emit();
        }
      });
    }
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  private save() {
    try {
      localStorage.setItem(KEY, JSON.stringify(this.state));
    } catch {
      /* ignore */
    }
    this.emit();
  }

  async get(): Promise<Settings> {
    return structuredClone(this.state);
  }

  async update(patch: Partial<Settings>): Promise<Settings> {
    this.state = { ...this.state, ...patch };
    this.save();
    return structuredClone(this.state);
  }

  async downloadModel(id: string): Promise<void> {
    const set = (patch: Partial<Settings["speechModels"][number]>) => {
      this.state = { ...this.state, speechModels: this.state.speechModels.map((m) => (m.id === id ? { ...m, ...patch } : m)) };
      this.save();
    };
    set({ state: "downloading", progress: 0 });
    let p = 0;
    const timer = setInterval(() => {
      p += 0.1;
      if (p >= 1) {
        clearInterval(timer);
        set({ state: "downloaded", progress: undefined });
      } else {
        set({ progress: p });
      }
    }, 500);
  }

  async testApiKey(_provider: ByokProvider, key: string) {
    await sleep(700);
    return key.trim().length >= 12
      ? { ok: true, message: "Connected. The key works." }
      : { ok: false, message: "That key didn’t work. Check it and try again." };
  }

  async deleteAllData(): Promise<void> {
    await sleep(400);
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
    this.state = { ...structuredClone(defaultSettings), onboardingDone: false, byokKeySet: false };
    this.save();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
