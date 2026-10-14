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
