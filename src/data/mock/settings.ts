import { defaultSettings } from "../defaultSettings";
import type { ByokProvider, Settings, SettingsStore } from "../settings";

export { defaultSettings };

const KEY = "talaan.mock.settings";

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

  private timers = new Map<string, ReturnType<typeof setInterval>>();

  private setModel(id: string, patch: { state: "not_downloaded" | "downloading" | "downloaded"; progress?: number } | { progress: number }) {
    this.state = {
      ...this.state,
      speechModels: this.state.speechModels.map((m) => (m.id === id ? { ...m, ...patch } : m)),
      localModels: this.state.localModels.map((m) => (m.id === id ? { ...m, ...patch } : m)),
    };
    this.save();
  }

  async downloadModel(id: string): Promise<void> {
    if (this.timers.has(id)) return;
    this.setModel(id, { state: "downloading", progress: 0 });
    let p = 0;
    const timer = setInterval(() => {
      p += 0.1;
      if (p >= 1) {
        clearInterval(timer);
        this.timers.delete(id);
        this.setModel(id, { state: "downloaded", progress: undefined });
      } else {
        this.setModel(id, { progress: p });
      }
    }, 500);
    this.timers.set(id, timer);
  }

  async cancelModelDownload(id: string): Promise<void> {
    clearInterval(this.timers.get(id));
    this.timers.delete(id);
    this.setModel(id, { state: "not_downloaded", progress: undefined });
  }

  async deleteModel(id: string): Promise<void> {
    this.setModel(id, { state: "not_downloaded", progress: undefined });
  }

  async testApiKey(_provider: ByokProvider, key: string) {
    await sleep(700);
    return key.trim().length >= 12
      ? { ok: true, message: "Connected. Summaries will use the recommended model." }
      : { ok: false, message: "That key didn’t work. Check it and try again." };
  }

  async clearApiKey(_provider: ByokProvider): Promise<void> {
    this.state = { ...this.state, byokKeySet: false };
    this.save();
  }

  async localServerModels(_address: string): Promise<string[]> {
    await sleep(300);
    return ["llama3.2:3b", "qwen3.5:4b"];
  }


  async dataInfo() {
    return { path: "~/Library/Application Support/Talaan", bytes: 1_200_000_000 };
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
