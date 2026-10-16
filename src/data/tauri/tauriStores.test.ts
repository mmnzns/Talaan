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

const MODELS = [
  { id: "whisper-turbo", name: "Whisper Large Turbo", maker: "OpenAI", note: "", engine: "whisper", sizeMb: 575, speed: 2, accuracy: 3, languages: "99 languages", state: "not_downloaded" },
  { id: "diarization", name: "Speaker separation", maker: "NVIDIA", note: "", engine: "diarization", sizeMb: 401, speed: 3, accuracy: 2, languages: "Any language", state: "downloaded" },
];

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

  it("sends Ask questions to Rust with their scope", async () => {
    const answer = { text: "Q1.", points: [], fromMeetings: 1, totalMeetings: 3 };
    invoke.mockResolvedValue(answer);
    const a = await new TauriMeetingStore().ask("pricing?", { kind: "tag", tag: "roadmap" });
    expect(invoke).toHaveBeenCalledWith("meetings_ask", { question: "pricing?", scope: { kind: "tag", tag: "roadmap" } });
    expect(a).toEqual(answer);
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

  it("also refreshes when a recording starts or stops, not on every timer tick", async () => {
    const s = new TauriMeetingStore();
    const fn = vi.fn();
    s.subscribe(fn);
    await Promise.resolve();
    fire("recorder-state", { state: "recording" });
    fire("recorder-state", { state: "recording" });
    expect(fn).not.toHaveBeenCalled();
    fire("recorder-state", { state: "idle" });
    expect(fn).toHaveBeenCalledTimes(1);
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
    expect((saved.value as Record<string, unknown>).speakerModel).toBeUndefined();
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
    expect((await store.get()).byokKeys).toEqual(["openai"]);
    expect((await store.get()).localModels[0].id).toBe("qwen3.5-4b");
  });

  it("each provider keeps its own saved key, and Remove keys clears them all", async () => {
    backend({ byokProvider: "claude" });
    const store = new TauriSettingsStore();
    await store.testApiKey("claude", "sk-ant");
    await store.update({ byokProvider: "openai" });
    await store.testApiKey("openai", "sk-oai");
    expect((await store.get()).byokKeys).toEqual(["claude", "openai"]);
    await store.clearApiKey("openai");
    expect((await store.get()).byokKeys).toEqual(["claude"]);
    await store.clearAllApiKeys();
    expect(invoke).toHaveBeenCalledWith("keys_clear_all", undefined);
    expect((await store.get()).byokKeys).toEqual([]);
  });

  it("a 1.0.0 key flag becomes a key for the provider it was saved with", async () => {
    backend({ byokProvider: "gemini", byokKeySet: true } as never);
    expect((await new TauriSettingsStore().get()).byokKeys).toEqual(["gemini"]);
  });

  it("call apps added in a later version join the user's list without touching their rules", async () => {
    backend({ callApps: [{ id: "zoom", name: "Zoom", note: "", rule: "never" }] });
    const apps = (await new TauriSettingsStore().get()).callApps;
    expect(apps[0]).toMatchObject({ id: "zoom", rule: "never" });
    expect(apps.map((a) => a.id)).toContain("discord");
    expect(apps.filter((a) => a.id === "zoom")).toHaveLength(1);
  });

  it("an unknown active model id falls back to the default", async () => {
    backend({ activeSpeechModel: "parakeet" });
    expect((await new TauriSettingsStore().get()).activeSpeechModel).toBe("whisper-turbo");
  });

  it("speech models come from disk, with live download progress", async () => {
    backend({});
    const store = new TauriSettingsStore();
    const first = await store.get();
    expect(first.speechModels[0].state).toBe("not_downloaded");
    // The speaker model comes in the same list but is kept apart from the speech models.
    expect(first.speechModels.map((m) => m.id)).toEqual(["whisper-turbo"]);
    expect(first.speakerModel.state).toBe("downloaded");
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

  it("connects a calendar address through Rust (secure key storage), never storing it in settings", async () => {
    backend({});
    const store = new TauriSettingsStore();
    expect("calendarIcsUrl" in (await store.get())).toBe(false);
    invoke.mockResolvedValueOnce(4);
    expect(await store.connectCalendar("https://example.com/basic.ics")).toBe(4);
    expect(invoke).toHaveBeenCalledWith("calendar_connect", { url: "https://example.com/basic.ics" });
    const sets = invoke.mock.calls.filter((c) => c[0] === "settings_set");
    const saved = sets[sets.length - 1][1] as { value: Record<string, unknown> };
    expect(saved.value.calendarConnected).toBe(true);
    expect(JSON.stringify(saved.value)).not.toContain("example.com");

    invoke.mockRejectedValueOnce("The calendar address didn't work (HTTP 404). Copy it again from your calendar.");
    await expect(store.connectCalendar("https://example.com/x")).rejects.toMatchObject({ message: "The calendar address didn't work (HTTP 404). Copy it again from your calendar." });

    await store.disconnectCalendar();
    expect(invoke).toHaveBeenCalledWith("calendar_disconnect", undefined);
    expect((await store.get()).calendarConnected).toBe(false);
  });

  it("drops the old plain-text calendar address and never writes it back", async () => {
    backend({ calendarConnected: true, calendarIcsUrl: "https://example.com/secret.ics" });
    const store = new TauriSettingsStore();
    expect("calendarIcsUrl" in (await store.get())).toBe(false);
    await store.update({ theme: "dark" });
    const sets = invoke.mock.calls.filter((c) => c[0] === "settings_set");
    const saved = sets[sets.length - 1][1] as { value: Record<string, unknown> };
    expect(JSON.stringify(saved.value)).not.toContain("secret.ics");
    expect(saved.value.calendarConnected).toBe(true);
  });

  it("deleting all data calls the backend", async () => {
    invoke.mockResolvedValue(undefined);
    await new TauriSettingsStore().deleteAllData();
    expect(invoke).toHaveBeenCalledWith("data_delete_all", undefined);
  });
});
