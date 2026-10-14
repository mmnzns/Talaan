import { defaultSettings } from "../defaultSettings";
import type { ByokProvider, Settings, SettingsStore } from "../settings";
import { call, onEvent } from "./invoke";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** SettingsStore backed by `settings.json`. Model downloads and key tests stay simulated until M4/M5. */
export class TauriSettingsStore implements SettingsStore {
  private state?: Settings;
  private listeners = new Set<() => void>();

  constructor() {
    onEvent("settings-changed", () => {
      this.state = undefined; // another window changed it: reload on next read
      this.emit();
    });
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  private async load(): Promise<Settings> {
    if (!this.state) {
      const stored = await call<Partial<Settings> | null>("settings_get");
      this.state = { ...structuredClone(defaultSettings), ...(stored ?? {}) };
    }
    return this.state;
  }

  async get(): Promise<Settings> {
    return structuredClone(await this.load());
  }

  async update(patch: Partial<Settings>): Promise<Settings> {
    const next = { ...(await this.load()), ...patch };
    this.state = next;
    await call<void>("settings_set", { value: next });
    this.emit();
    return structuredClone(next);
  }

  async downloadModel(id: string): Promise<void> {
    const set = async (p: Partial<Settings["speechModels"][number]>) => {
      const s = await this.load();
      await this.update({ speechModels: s.speechModels.map((m) => (m.id === id ? { ...m, ...p } : m)) });
    };
    await set({ state: "downloading", progress: 0 });
    let p = 0;
    const timer = setInterval(() => {
      p += 0.1;
      void (p >= 1 ? (clearInterval(timer), set({ state: "downloaded", progress: undefined })) : set({ progress: p }));
    }, 500);
  }

  async testApiKey(_provider: ByokProvider, key: string) {
    await sleep(700);
    return key.trim().length >= 12
      ? { ok: true, message: "Connected. The key works." }
      : { ok: false, message: "That key didn’t work. Check it and try again." };
  }

  dataInfo(): Promise<{ path: string; bytes: number }> {
    return call("data_info");
  }

  async deleteAllData(): Promise<void> {
    await call<void>("data_delete_all"); // the app restarts into first-run
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
