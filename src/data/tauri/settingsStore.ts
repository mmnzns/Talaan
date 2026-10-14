import { defaultSettings } from "../defaultSettings";
import type { ByokProvider, Settings, SettingsStore, SpeechModel } from "../settings";
import { call, onEvent } from "./invoke";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * SettingsStore backed by `settings.json`. Speech models are not stored there: their list and state come from
 * what is on disk (`models_list`), so a settings file can never claim a model that isn't there.
 * Key tests stay simulated until M5.
 */
export class TauriSettingsStore implements SettingsStore {
  private state?: Settings;
  private models?: SpeechModel[];
  private listeners = new Set<() => void>();

  constructor() {
    onEvent("settings-changed", () => {
      this.state = undefined; // another window changed it: reload on next read
      this.emit();
    });
    onEvent("models-changed", () => {
      this.models = undefined;
      this.emit();
    });
    onEvent<{ id: string; progress: number }>("model-download", ({ id, progress }) => {
      if (!this.models) return;
      this.models = this.models.map((m) => (m.id === id ? { ...m, state: "downloading", progress } : m));
      this.emit();
    });
  }

  private async loadModels(): Promise<SpeechModel[]> {
    if (!this.models) this.models = await call<SpeechModel[]>("models_list");
    return this.models;
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  private async load(): Promise<Settings> {
    if (!this.state) {
      const stored = await call<Partial<Settings> | null>("settings_get");
      // No settings file yet means this is the first launch: show onboarding.
      this.state = stored ? { ...structuredClone(defaultSettings), ...stored } : { ...structuredClone(defaultSettings), onboardingDone: false };
    }
    return this.state;
  }

  async get(): Promise<Settings> {
    const [s, speechModels] = await Promise.all([this.load(), this.loadModels()]);
    // A model id that no longer exists (older builds used other ids) means the default, as Rust does.
    const activeSpeechModel = speechModels.some((m) => m.id === s.activeSpeechModel) ? s.activeSpeechModel : defaultSettings.activeSpeechModel;
    return structuredClone({ ...s, speechModels, activeSpeechModel });
  }

  async update(patch: Partial<Settings>): Promise<Settings> {
    const next = { ...(await this.load()), ...patch };
    this.state = next;
    const { speechModels: _ignored, ...stored } = next;
    await call<void>("settings_set", { value: stored });
    this.emit();
    return structuredClone({ ...next, speechModels: await this.loadModels() });
  }

  downloadModel(id: string): Promise<void> {
    return call<void>("models_download", { id });
  }

  cancelModelDownload(id: string): Promise<void> {
    return call<void>("models_cancel", { id });
  }

  deleteModel(id: string): Promise<void> {
    return call<void>("models_delete", { id });
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
