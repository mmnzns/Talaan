import { describe, expect, it, vi } from "vitest";
import { defaultSettings } from "../data/defaultSettings";
import { seedMeetings } from "../data/mock/seed";
import type { AutoSaveStatus, Settings } from "../data/settings";
import type { Meeting } from "../data/types";
import { retrySkipped, saveCopy, startAutoSave } from "./autoSave";
import { toMarkdown } from "./share";

const meetings = seedMeetings(new Date("2026-10-14T20:00:00"));
const q4 = meetings.find((m) => m.id === "m-q4")!;

function fakes(patch: Partial<Settings> = {}, status: AutoSaveStatus | null = { reachable: true, skipped: [] }) {
  const all = new Map<string, Meeting>(meetings.map((m) => [m.id, m]));
  const written: { id: string; contents: string; finished: boolean }[] = [];
  const store = { getMeeting: vi.fn(async (id: string) => all.get(id)) };
  const settings = {
    get: async () => ({ ...defaultSettings, autoSaveFolder: "/notes", ...patch }),
    writeAutoSave: vi.fn(async (id: string, contents: string, finished: boolean) => {
      written.push({ id, contents, finished });
      return "written" as const;
    }),
    autoSaveStatus: vi.fn(async () => status),
  };
  return { store, settings, written, all };
}

describe("saveCopy", () => {
  it("writes the same Markdown as Export", async () => {
    const f = fakes();
    expect(await saveCopy(f.store, f.settings, "m-q4", true)).toBe("written");
    expect(f.written).toEqual([{ id: "m-q4", contents: toMarkdown(q4, false), finished: true }]);
  });

  it("adds the transcript when asked, and always when there is no summary", async () => {
    const f = fakes({ autoSaveTranscript: true });
    await saveCopy(f.store, f.settings, "m-q4", false);
    expect(f.written[0].contents).toBe(toMarkdown(q4, true));

    const g = fakes();
    g.all.set("m-q4", { ...q4, summary: undefined });
    await saveCopy(g.store, g.settings, "m-q4", true);
    expect(g.written[0].contents).toContain(toMarkdown({ ...q4, summary: undefined }, true));
  });

  it("does nothing when no folder is chosen or the meeting isn't finished", async () => {
    const off = fakes({ autoSaveFolder: "" });
    expect(await saveCopy(off.store, off.settings, "m-q4", true)).toBe("off");
    const busy = fakes();
    busy.all.set("m-q4", { ...q4, status: "summarizing" });
    expect(await saveCopy(busy.store, busy.settings, "m-q4", true)).toBe("not_ready");
    expect(off.written.length + busy.written.length).toBe(0);
  });
});

describe("retrySkipped", () => {
  it("catches up on skipped meetings once the folder is back", async () => {
    const f = fakes({}, { reachable: true, skipped: ["m-q4", "m-1on1"] });
    expect(await retrySkipped(f.store, f.settings)).toBe(2);
    expect(f.written.map((w) => w.id)).toEqual(["m-q4", "m-1on1"]);
  });

  it("waits while the folder is still away", async () => {
    const f = fakes({}, { reachable: false, skipped: ["m-q4"] });
    expect(await retrySkipped(f.store, f.settings)).toBe(0);
    expect(f.written).toEqual([]);
  });
});

describe("startAutoSave", () => {
  it("writes each due copy in order", async () => {
    const f = fakes();
    let fire: (d: { id: string; finished: boolean }) => void = () => {};
    const off = vi.fn();
    const stop = startAutoSave(f.store, f.settings, (cb) => {
      fire = cb;
      return off;
    });
    fire({ id: "m-q4", finished: true });
    fire({ id: "m-1on1", finished: false });
    await vi.waitFor(() => expect(f.written.map((w) => [w.id, w.finished])).toEqual([["m-q4", true], ["m-1on1", false]]));
    stop();
    expect(off).toHaveBeenCalled();
  });
});
