import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listeners = new Map<string, () => void>();
const unlisten = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, cb: () => void) => {
    listeners.set(event, cb);
    return Promise.resolve(unlisten);
  },
}));

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
    listeners.get("meetings-changed")!();
    expect(fn).toHaveBeenCalledTimes(1);
    off();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalled();
  });
});

describe("TauriSettingsStore", () => {
  it("merges stored settings over the defaults", async () => {
    invoke.mockResolvedValue({ theme: "dark" });
    const s = await new TauriSettingsStore().get();
    expect(s.theme).toBe("dark");
    expect(s.retention).toBe("7d");
  });

  it("saves the whole object on update and tells subscribers", async () => {
    invoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "settings_get" ? null : undefined));
    const store = new TauriSettingsStore();
    const fn = vi.fn();
    store.subscribe(fn);
    const next = await store.update({ retention: "30d" });
    expect(next.retention).toBe("30d");
    const saved = invoke.mock.calls.find((c) => c[0] === "settings_set")![1] as { value: { retention: string; theme: string } };
    expect(saved.value.retention).toBe("30d");
    expect(saved.value.theme).toBe("system");
    expect(fn).toHaveBeenCalled();
  });

  it("reloads when another window changes the settings", async () => {
    invoke.mockResolvedValueOnce({ theme: "light" }).mockResolvedValueOnce({ theme: "dark" });
    const store = new TauriSettingsStore();
    expect((await store.get()).theme).toBe("light");
    listeners.get("settings-changed")!();
    expect((await store.get()).theme).toBe("dark");
  });

  it("deleting all data calls the backend", async () => {
    invoke.mockResolvedValue(undefined);
    await new TauriSettingsStore().deleteAllData();
    expect(invoke).toHaveBeenCalledWith("data_delete_all", undefined);
  });
});
