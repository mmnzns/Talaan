import { defaultSettings, withDefaults } from "../defaultSettings";
import { onWindows } from "../../lib/platform";
import type { ByokProvider, GpuStatus, Settings, SettingsStore } from "../settings";

export { defaultSettings };

const KEY = "talaan.mock.settings";

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return withDefaults(JSON.parse(raw));
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

  /** The settings right now, for other mock stores (no copy; don't change it). */
  current(): Settings {
    return this.state;
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

  private setModel(id: string, patch: { state: "not_downloaded" | "downloading" | "downloaded"; progress?: number; partial?: number; error?: string; cancelled?: boolean } | { progress: number }) {
    this.state = {
      ...this.state,
      speechModels: this.state.speechModels.map((m) => (m.id === id ? { ...m, ...patch } : m)),
      localModels: this.state.localModels.map((m) => (m.id === id ? { ...m, ...patch } : m)),
      speakerModel: this.state.speakerModel.id === id ? { ...this.state.speakerModel, ...patch } : this.state.speakerModel,
      searchModel: this.state.searchModel.id === id ? { ...this.state.searchModel, ...patch } : this.state.searchModel,
    };
    this.save();
  }

  async downloadModel(id: string): Promise<void> {
    if (this.timers.has(id)) return;
    const all = [...this.state.speechModels, ...this.state.localModels, this.state.speakerModel, this.state.searchModel];
    // Resumes from what a cancelled (or failed) download left, like the real one.
    let p = all.find((m) => m.id === id)?.partial ?? 0;
    this.setModel(id, { state: "downloading", progress: p, partial: undefined, error: undefined, cancelled: undefined });
    // `?dl=fail` in the browser preview: downloads stop at 40% with an error.
    const fail = typeof location !== "undefined" && new URLSearchParams(location.search).get("dl") === "fail";
    // Big models take longer, like real ones (a 1 GB model takes 5 s here).
    const step = 0.1 * Math.min(1, 1000 / (all.find((m) => m.id === id)?.sizeMb ?? 1000));
    const timer = setInterval(() => {
      p += step;
      if (fail && p >= 0.4) {
        clearInterval(timer);
        this.timers.delete(id);
        this.setModel(id, { state: "not_downloaded", progress: undefined, partial: p, error: "The download of the model stopped: no data for 30 seconds. Check your connection and try again." });
      } else if (p >= 1) {
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
    const m = [...this.state.speechModels, ...this.state.localModels].find((x) => x.id === id);
    // The part already downloaded stays (Download resumes), like the real one.
    this.setModel(id, { state: "not_downloaded", progress: undefined, partial: m?.progress || undefined, cancelled: true });
  }

  async connectNotion(secret: string) {
    await sleep(500);
    if (!secret.trim().startsWith("ntn_") && !secret.trim().startsWith("secret_")) throw new Error("Notion didn't accept that secret.");
    await this.update({ notion: { connected: true, workspace: "Sample workspace" } });
    return "Sample workspace";
  }

  async disconnectNotion() {
    await this.update({ notion: { connected: false } });
  }

  async notionDestinations() {
    return [
      { id: "db-notes", kind: "database" as const, title: "Meeting Notes" },
      { id: "pg-journal", kind: "page" as const, title: "Work journal" },
    ];
  }

  async browsersStatus() {
    return [
      { name: "Chrome", state: "allowed" as const },
      { name: "Safari", state: "not_asked" as const },
      { name: "Arc", state: "not_installed" as const },
    ];
  }

  async audioInputs() {
    return ["Built-in microphone", "Headset microphone"];
  }

  async checkForUpdate() {
    await sleep(600);
    return null;
  }

  async installUpdate() {}

  async calendarAccess() {
    return "allowed" as const;
  }

  async requestCalendar() {
    return "allowed" as const;
  }

  async connectCalendar(url: string) {
    await sleep(600);
    if (!/^(https?|webcals?):\/\//i.test(url.trim())) throw new Error("That doesn't look like a calendar address. Copy the whole address, starting with https://.");
    await this.update({ calendarConnected: true });
    return 7;
  }

  async disconnectCalendar() {
    await this.update({ calendarConnected: false });
  }

  async deleteModel(id: string): Promise<void> {
    this.setModel(id, { state: "not_downloaded", progress: undefined });
  }

  async testApiKey(provider: ByokProvider, key: string) {
    await sleep(700);
    if (key.trim().length < 12) return { ok: false, message: "That key didn’t work. Check it and try again." };
    if (!this.state.byokKeys.includes(provider)) this.state = { ...this.state, byokKeys: [...this.state.byokKeys, provider] };
    this.save();
    return { ok: true, message: "Connected. Summaries will use the recommended model." };
  }

  async clearApiKey(provider: ByokProvider): Promise<void> {
    this.state = { ...this.state, byokKeys: this.state.byokKeys.filter((p) => p !== provider) };
    this.save();
  }

  async clearAllApiKeys(): Promise<void> {
    this.state = { ...this.state, byokKeys: [] };
    this.save();
  }

  async localServerModels(_address: string): Promise<string[]> {
    await sleep(300);
    return ["llama3.2:3b", "qwen3.5:4b"];
  }


  async gpuStatus(): Promise<GpuStatus> {
    await sleep(200);
    const s = await this.get();
    if (!s.useGpu) return { state: "off" };
    return onWindows ? { state: "in_use", device: "NVIDIA GeForce RTX 5070 Ti", vramMb: 16303 } : { state: "metal" };
  }

  async retryGpu(): Promise<GpuStatus> {
    return this.gpuStatus();
  }

  async dataInfo() {
    return { path: "~/Library/Application Support/Talaan", bytes: 1_200_000_000 };
  }

  async revealData(): Promise<void> {}

  async deleteAllData(): Promise<void> {
    await sleep(400);
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* ignore */
    }
    this.state = { ...structuredClone(defaultSettings), onboardingDone: false, byokKeys: [] };
    this.save();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
