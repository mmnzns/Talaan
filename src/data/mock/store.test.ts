import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockMeetingStore } from "./store";

const NOW = new Date("2026-10-14T20:00:00");
let store: MockMeetingStore;

beforeEach(() => {
  vi.useRealTimers();
  store = new MockMeetingStore(NOW, { askDelayMs: 0 });
});

describe("listMeetings", () => {
  it("returns meetings newest first", async () => {
    const list = await store.listMeetings();
    const times = list.map((m) => Date.parse(m.startedAt));
    expect(times).toEqual([...times].sort((a, b) => b - a));
  });

  it("filters by folder", async () => {
    const list = await store.listMeetings({ folderId: "clients" });
    expect(list.map((m) => m.id)).toEqual(["m-import"]);
  });

  it("filters by tag", async () => {
    const list = await store.listMeetings({ tag: "1:1" });
    expect(list.map((m) => m.id)).toEqual(["m-1on1"]);
  });

  it("searches titles, summaries and transcripts", async () => {
    expect((await store.listMeetings({ query: "vendor demo" })).map((m) => m.id)).toEqual(["m-vendor"]);
    expect((await store.listMeetings({ query: "bigger leak" })).map((m) => m.id)).toContain("m-q4");
    expect((await store.listMeetings({ query: "harbour" })).map((m) => m.id)).toEqual(["m-import"]);
    expect(await store.listMeetings({ query: "zzzz-nothing" })).toEqual([]);
  });
});

describe("edits", () => {
  it("renames a meeting", async () => {
    await store.renameMeeting("m-q4", "Roadmap");
    expect((await store.getMeeting("m-q4"))?.title).toBe("Roadmap");
  });

  it("pins and deletes", async () => {
    await store.setPinned("m-q4", true);
    expect((await store.getMeeting("m-q4"))?.pinned).toBe(true);
    await store.deleteMeeting("m-q4");
    expect(await store.getMeeting("m-q4")).toBeUndefined();
  });

  it("renames a speaker everywhere it is displayed", async () => {
    await store.renameSpeaker("m-q4", "them:1", "Dana R.");
    expect((await store.getMeeting("m-q4"))?.speakers["them:1"]).toBe("Dana R.");
  });

  it("edits a transcript line and a summary, then reverts both", async () => {
    const before = await store.getMeeting("m-q4");
    const line = before!.transcript[0];
    await store.updateTranscriptLine("m-q4", line.id, "Fixed text");
    await store.updateSummary("m-q4", { ...before!.summary!, recap: "New recap", edited: true });

    const edited = await store.getMeeting("m-q4");
    expect(edited!.transcript[0]).toMatchObject({ text: "Fixed text", edited: true });
    expect(edited!.summary).toMatchObject({ recap: "New recap", edited: true });

    await store.revertEdits("m-q4");
    const reverted = await store.getMeeting("m-q4");
    expect(reverted!.transcript[0]).toMatchObject({ text: line.text, edited: false });
    expect(reverted!.summary).toMatchObject({ recap: before!.summary!.recap, edited: false });
  });

  it("sets tags and moves between folders", async () => {
    await store.setTags("m-q4", ["roadmap", "q4"]);
    await store.moveToFolder("m-q4", "clients");
    const m = await store.getMeeting("m-q4");
    expect(m?.tags).toEqual(["roadmap", "q4"]);
    expect(m?.folderId).toBe("clients");
    expect(await store.listTags()).toContain("q4");
  });
});

describe("folders", () => {
  it("counts meetings per folder", async () => {
    const folders = await store.listFolders();
    expect(folders.find((f) => f.id === "work")?.count).toBe(4);
    expect(folders.find((f) => f.id === "clients")?.count).toBe(1);
  });
});

describe("background work", () => {
  beforeEach(() => vi.useFakeTimers());

  it("generates a summary for a transcript-only meeting", async () => {
    expect((await store.getMeeting("m-import"))?.summary).toBeUndefined();
    await store.generateSummary("m-import");
    expect((await store.getMeeting("m-import"))?.status).toBe("summarizing");
    await vi.advanceTimersByTimeAsync(3000);
    const m = await store.getMeeting("m-import");
    expect(m?.status).toBe("ready");
    expect(m?.summary?.recap).toBeTruthy();
    expect(m?.progress).toBeUndefined();
  });

  it("recovers an interrupted recording into a transcript", async () => {
    await store.recoverMeeting("m-recover");
    expect((await store.getMeeting("m-recover"))?.status).toBe("transcribing");
    await vi.advanceTimersByTimeAsync(3000);
    const m = await store.getMeeting("m-recover");
    expect(m?.status).toBe("ready");
    expect(m?.transcript.length).toBeGreaterThan(0);
  });

  it("retries a failed meeting", async () => {
    await store.retryProcessing("m-failed");
    await vi.advanceTimersByTimeAsync(3000);
    expect((await store.getMeeting("m-failed"))?.status).toBe("ready");
  });
});

describe("createFolder", () => {
  it("adds a folder with a unique id", async () => {
    const a = await store.createFolder("Q4 Planning");
    const b = await store.createFolder("Q4 Planning");
    expect(a.id).toBe("q4-planning");
    expect(b.id).toBe("q4-planning-2");
    expect((await store.listFolders()).map((f) => f.id)).toContain("q4-planning");
  });

  it("renames and deletes a folder, keeping its meetings", async () => {
    const f = await store.createFolder("Clients");
    const [m] = await store.listMeetings();
    await store.moveToFolder(m.id, f.id);
    await store.renameFolder(f.id, "Client calls");
    expect((await store.listFolders()).find((x) => x.id === f.id)?.name).toBe("Client calls");
    await store.deleteFolder(f.id);
    expect((await store.listFolders()).some((x) => x.id === f.id)).toBe(false);
    expect((await store.getMeeting(m.id))?.folderId).toBeUndefined();
  });
});

describe("empty store", () => {
  it("starts with no meetings", async () => {
    const empty = new MockMeetingStore(NOW, { empty: true });
    expect(await empty.listMeetings()).toEqual([]);
  });
});

describe("ask", () => {
  it("answers a pricing question with citations", async () => {
    const a = await store.ask("What did we decide about pricing?", { kind: "all" });
    expect(a.points.length).toBeGreaterThan(0);
    expect(a.points.every((p) => p.citation)).toBe(true);
  });

  it("returns an empty answer when nothing matches", async () => {
    const a = await store.ask("Tell me about unicorns", { kind: "all" });
    expect(a.points).toEqual([]);
  });
});

describe("subscribe", () => {
  it("fires on change and stops after unsubscribe", async () => {
    const fn = vi.fn();
    const off = store.subscribe(fn);
    await store.renameMeeting("m-q4", "A");
    expect(fn).toHaveBeenCalledTimes(1);
    off();
    await store.renameMeeting("m-q4", "B");
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
