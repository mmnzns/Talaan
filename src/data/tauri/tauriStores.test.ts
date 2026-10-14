import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listeners = new Map<string, (e: { payload: unknown }) => void>();
const unlisten = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(event, cb);
    return Promise.resolve(unlisten);
  },
}));
const fire = (event: string, payload?: unknown) => listeners.get(event)!({ payload });

const MODELS = [{ id: "whisper-turbo", name: "Whisper Large Turbo", maker: "OpenAI", note: "", engine: "whisper", sizeMb: 575, speed: 2, accuracy: 3, languages: "99 languages", state: "not_downloaded" }];

/** Answers `settings_get` with each of `settings` in turn, `models_list` with MODELS, everything else with undefined. */
function backend(...settings: unknown[]) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "settings_get") return Promise.resolve(settings.length > 1 ? settings.shift() : settings[0]);
    if (cmd === "models_list") return Promise.resolve(structuredClone(MODELS));
    if (cmd === "llm_models_list") return Promise.resolve([{ id: "qwen3.5-4b", choice: "recommended", name: "Qwen3.5 4B", note: "", sizeMb: 2741, state: "not_downloaded" }]);
    if (cmd === "keys_save") return Promise.resolve({ ok: true, message: "Connected." });
    return Promise.resolve(undefined);
  });
}

import { StoreError } from "../errors";
import { TauriMeetingStore } from "./meetingStore";
import { TauriSettingsStore } from "./settingsStore";

beforeEach(() => {
  invoke.mockReset();
  listeners.clear();
  unlisten.mockReset();
});

describe("TauriMeetingStore", () => {
  it("sends camelCase arguments to the right commands", async () => {
    invoke.mockResolvedValue(undefined);
    const s = new TauriMeetingStore();
    await s.renameMeeting("m1", "New");
    await s.updateTranscriptLine("m1", "l0", "text");
    await s.moveToFolder("m1", undefined);
    await s.setTags("m1", ["a"]);
    expect(invoke.mock.calls).toEqual([
      ["meetings_rename", { id: "m1", title: "New" }],
      ["meetings_update_transcript_line", { id: "m1", lineId: "l0", text: "text" }],
      ["meetings_move_to_folder", { id: "m1", folderId: null }],
      ["meetings_set_tags", { id: "m1", tags: ["a"] }],
    ]);
  });

  it("maps a missing meeting (null) to undefined", async () => {
    invoke.mockResolvedValue(null);
    expect(await new TauriMeetingStore().getMeeting("nope")).toBeUndefined();
  });

  it("turns Rust errors into StoreError", async () => {
    invoke.mockRejectedValue({ code: "not_yet_available", message: "needs M5" });
    await expect(new TauriMeetingStore().generateSummary("m1")).rejects.toMatchObject({ code: "not_yet_available" });
    await expect(new TauriMeetingStore().generateSummary("m1")).rejects.toBeInstanceOf(StoreError);
  });

  it("answers Ask honestly until M8", async () => {
    invoke.mockResolvedValue([{}, {}, {}]);
    const a = await new TauriMeetingStore().ask("anything", { kind: "all" });
    expect(a).toEqual({ text: "", points: [], fromMeetings: 0, totalMeetings: 3 });
  });

  it("subscribes to meetings-changed and unsubscribes", async () => {
    const s = new TauriMeetingStore();
    const fn = vi.fn();
    const off = s.subscribe(fn);
    await Promise.resolve();
    fire("meetings-changed");
    expect(fn).toHaveBeenCalledTimes(1);
    off();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalled();
  });
});

describe("TauriSettingsStore", () => {
  it("merges stored settings over the defaults", async () => {
    backend({ theme: "dark" });
    const s = await new TauriSettingsStore().get();
    expect(s.theme).toBe("dark");
    expect(s.retention).toBe("7d");
  });

  it("treats a missing settings file as the first launch", async () => {
    backend(null);
    expect((await new TauriSettingsStore().get()).onboardingDone).toBe(false);
  });

  it("saves the whole object on update and tells subscribers", async () => {
    backend(null);
    const store = new TauriSettingsStore();
    const fn = vi.fn();
    store.subscribe(fn);
    const next = await store.update({ retention: "30d" });
    expect(next.retention).toBe("30d");
    const saved = invoke.mock.calls.find((c) => c[0] === "settings_set")![1] as { value: { retention: string; theme: string; speechModels?: unknown } };
    expect(saved.value.retention).toBe("30d");
    expect(saved.value.theme).toBe("system");
    // Model state lives on disk, never in settings.json.
    expect(saved.value.speechModels).toBeUndefined();
    expect(fn).toHaveBeenCalled();
  });

  it("reloads when another window changes the settings", async () => {
    backend({ theme: "light" }, { theme: "dark" });
    const store = new TauriSettingsStore();
    expect((await store.get()).theme).toBe("light");
    fire("settings-changed");
    expect((await store.get()).theme).toBe("dark");
  });

  it("a working key is saved through the Keychain command, never in settings", async () => {
    backend({ byokProvider: "openai" });
    const store = new TauriSettingsStore();
    const r = await store.testApiKey("openai", "sk-live");
    expect(r.ok).toBe(true);
    expect(invoke).toHaveBeenCalledWith("keys_save", { provider: "openai", key: "sk-live", baseUrl: null });
    const saved = invoke.mock.calls.filter((c) => c[0] === "settings_set").map((c) => JSON.stringify(c[1]));
    expect(saved.some((v) => v.includes("sk-live"))).toBe(false);
    expect((await store.get()).byokKeySet).toBe(true);
    expect((await store.get()).localModels[0].id).toBe("qwen3.5-4b");
  });

  it("an unknown active model id falls back to the default", async () => {
    backend({ activeSpeechModel: "parakeet" });
    expect((await new TauriSettingsStore().get()).activeSpeechModel).toBe("whisper-turbo");
  });

  it("speech models come from disk, with live download progress", async () => {
    backend({});
    const store = new TauriSettingsStore();
    expect((await store.get()).speechModels[0].state).toBe("not_downloaded");
    const fn = vi.fn();
    store.subscribe(fn);
    fire("model-download", { id: "whisper-turbo", progress: 0.4 });
    const m = (await store.get()).speechModels[0];
    expect([m.state, m.progress]).toEqual(["downloading", 0.4]);
    expect(fn).toHaveBeenCalled();
    await store.downloadModel("whisper-turbo");
    expect(invoke).toHaveBeenCalledWith("models_download", { id: "whisper-turbo" });
    // When a download ends Rust says so and the list is read again.
    fire("models-changed");
    await store.get();
    expect(invoke.mock.calls.filter((c) => c[0] === "models_list")).toHaveLength(2);
  });

  it("deleting all data calls the backend", async () => {
    invoke.mockResolvedValue(undefined);
    await new TauriSettingsStore().deleteAllData();
    expect(invoke).toHaveBeenCalledWith("data_delete_all", undefined);
  });
});
