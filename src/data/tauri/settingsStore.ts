import { defaultSettings, withDefaults } from "../defaultSettings";
import type { BrowserState, ByokProvider, CalendarAccess, LocalModel, NotionDestination, Settings, SettingsStore, SpeechModel } from "../settings";
import { call, onEvent } from "./invoke";


/**
 * SettingsStore backed by `settings.json`. Speech models are not stored there: their list and state come from
 * what is on disk (`models_list`), so a settings file can never claim a model that isn't there.
 * Key tests stay simulated until M5.
 */
export class TauriSettingsStore implements SettingsStore {
  private state?: Settings;
  private models?: SpeechModel[];
  private speaker?: SpeechModel;
  private search?: SpeechModel;
  private localModels?: LocalModel[];
  private listeners = new Set<() => void>();

  constructor() {
    onEvent("settings-changed", () => {
      this.state = undefined; // another window changed it: reload on next read
      this.emit();
    });
    onEvent("models-changed", () => {
      this.models = undefined;
      this.speaker = undefined;
      this.search = undefined;
      this.localModels = undefined;
      this.emit();
    });
    onEvent<{ id: string; progress: number }>("model-download", ({ id, progress }) => {
      this.models = this.models?.map((m) => (m.id === id ? { ...m, state: "downloading", progress } : m));
      if (this.speaker?.id === id) this.speaker = { ...this.speaker, state: "downloading", progress };
      if (this.search?.id === id) this.search = { ...this.search, state: "downloading", progress };
      this.localModels = this.localModels?.map((m) => (m.id === id ? { ...m, state: "downloading", progress } : m));
      this.emit();
    });
  }

  private async loadModels(): Promise<SpeechModel[]> {
    if (!this.models) {
      // One list from Rust: the speech models plus the speaker separation and search models.
      const all = await call<SpeechModel[]>("models_list");
      this.models = all.filter((m) => m.engine === "whisper" || m.engine === "parakeet");
      this.speaker = all.find((m) => m.engine === "diarization");
      this.search = all.find((m) => m.engine === "search");
    }
    return this.models;
  }

  private extraModels() {
    return { speakerModel: this.speaker ?? defaultSettings.speakerModel, searchModel: this.search ?? defaultSettings.searchModel };
  }

  private async loadLocalModels(): Promise<LocalModel[]> {
    if (!this.localModels) this.localModels = await call<LocalModel[]>("llm_models_list");
    return this.localModels;
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  private async load(): Promise<Settings> {
    if (!this.state) {
      const stored = await call<Partial<Settings> | null>("settings_get");
      // No settings file yet means this is the first launch: show onboarding.
      this.state = stored ? withDefaults(stored) : { ...structuredClone(defaultSettings), onboardingDone: false };
    }
    return this.state;
  }

  async get(): Promise<Settings> {
    const [s, speechModels, localModels] = await Promise.all([this.load(), this.loadModels(), this.loadLocalModels()]);
    // A model id that no longer exists (older builds used other ids) means the default, as Rust does.
    const activeSpeechModel = speechModels.some((m) => m.id === s.activeSpeechModel) ? s.activeSpeechModel : defaultSettings.activeSpeechModel;
    return structuredClone({ ...s, speechModels, ...this.extraModels(), localModels, activeSpeechModel });
  }

  async update(patch: Partial<Settings>): Promise<Settings> {
    const next = { ...(await this.load()), ...patch };
    this.state = next;
    const { speechModels: _speech, speakerModel: _speaker, searchModel: _search, localModels: _local, ...stored } = next;
    await call<void>("settings_set", { value: stored });
    this.emit();
    const speechModels = await this.loadModels();
    return structuredClone({ ...next, speechModels, ...this.extraModels(), localModels: await this.loadLocalModels() });
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

  async testApiKey(provider: ByokProvider, key: string) {
    const s = await this.load();
    const r = await call<{ ok: boolean; message: string; model?: string }>("keys_save", { provider, key, baseUrl: s.byokBaseUrl ?? null });
    if (r.ok) await this.update({ byokKeySet: true });
    return r;
  }

  async clearApiKey(provider: ByokProvider): Promise<void> {
    await call<void>("keys_clear", { provider });
    await this.update({ byokKeySet: false });
  }

  localServerModels(address: string): Promise<string[]> {
    return call<string[]>("local_server_models", { address });
  }

  async connectNotion(secret: string): Promise<string> {
    const { workspace } = await call<{ workspace: string }>("notion_connect", { secret });
    await this.update({ notion: { connected: true, workspace } });
    return workspace;
  }

  async disconnectNotion(): Promise<void> {
    await call<void>("notion_disconnect");
    await this.update({ notion: { connected: false } });
  }

  notionDestinations(): Promise<NotionDestination[]> {
    return call<NotionDestination[]>("notion_destinations");
  }

  browsersStatus(): Promise<{ name: string; state: BrowserState }[]> {
    return call("browsers_status");
  }

  audioInputs() {
    return call<string[]>("audio_inputs");
  }

  checkForUpdate() {
    return call<{ version: string; notes?: string } | null>("updates_check");
  }

  installUpdate() {
    return call<void>("updates_install");
  }

  calendarAccess(): Promise<CalendarAccess> {
    return call<CalendarAccess>("calendar_access");
  }

  requestCalendar(): Promise<CalendarAccess> {
    return call<CalendarAccess>("calendar_request");
  }

  testCalendarIcs(url: string): Promise<number> {
    return call<number>("calendar_test_ics", { url });
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
