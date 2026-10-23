import { describe, expect, it, vi } from "vitest";
import { baseName, ImportQueue, partitionImports, type BatchSummary, type ImportOutcome, type QueueSnapshot } from "./importQueue";

const EXT = ["m4a", "mp3", "wav", "flac"];

describe("baseName", () => {
  it("handles Windows and Mac paths", () => {
    expect(baseName("C:\\Users\\me\\Voice Memos\\call 1.m4a")).toBe("call 1.m4a");
    expect(baseName("/Users/me/Voice Memos/call 1.m4a")).toBe("call 1.m4a");
    expect(baseName("call.mp3")).toBe("call.mp3");
  });
});

describe("partitionImports", () => {
  it("keeps the audio files in order and names the rest", () => {
    const r = partitionImports(["/a/one.MP3", "/a/notes.docx", "/a/two.m4a", "/a/folder", "/a/three.wav"], EXT);
    expect(r.accepted).toEqual(["/a/one.MP3", "/a/two.m4a", "/a/three.wav"]);
    expect(r.unsupported).toEqual(["/a/notes.docx", "/a/folder"]);
  });
  it("imports the same path once", () => {
    expect(partitionImports(["/a/x.mp3", "/a/x.mp3"], EXT).accepted).toEqual(["/a/x.mp3"]);
  });
  it("returns nothing for nothing", () => {
    expect(partitionImports([], EXT)).toEqual({ accepted: [], unsupported: [] });
  });
});

function setup(importOne: (path: string, name: string) => Promise<ImportOutcome>) {
  const snapshots: QueueSnapshot[] = [];
  let done!: (s: BatchSummary) => void;
  const finished = new Promise<BatchSummary>((r) => (done = r));
  const onFinish = vi.fn((s: BatchSummary) => done(s));
  const queue = new ImportQueue({ importOne, onChange: (s) => snapshots.push(s), onFinish });
  return { queue, snapshots, finished, onFinish };
}

describe("ImportQueue", () => {
  it("imports one after another, never two at once", async () => {
    let inFlight = 0;
    let maxInFlight = 0;
    const order: string[] = [];
    const { queue, finished } = setup(async (path) => {
      inFlight++;
      maxInFlight = Math.max(maxInFlight, inFlight);
      order.push(path);
      await new Promise((r) => setTimeout(r, 1));
      inFlight--;
      return { ok: true, id: `id-${order.length}` };
    });
    queue.add(["/a/1.mp3", "/a/2.mp3", "/a/3.mp3"]);
    const summary = await finished;
    expect(order).toEqual(["/a/1.mp3", "/a/2.mp3", "/a/3.mp3"]);
    expect(maxInFlight).toBe(1);
    expect(summary).toEqual({ total: 3, imported: ["id-1", "id-2", "id-3"], failures: [] });
  });

  it("keeps going after a failure and says which file failed and why", async () => {
    const { queue, finished } = setup(async (path) => (path.includes("bad") ? { ok: false, message: "Could not read this file" } : { ok: true, id: path }));
    queue.add(["/a/1.mp3", "/a/bad.mp3", "/a/3.mp3"]);
    const summary = await finished;
    expect(summary.imported).toEqual(["/a/1.mp3", "/a/3.mp3"]);
    expect(summary.failures).toEqual([{ name: "bad.mp3", message: "Could not read this file" }]);
    expect(summary.total).toBe(3);
  });

  it("counts a throw as that file's failure", async () => {
    const { queue, finished } = setup(async (path) => {
      if (path.includes("boom")) throw new Error("exploded");
      return { ok: true, id: "ok" };
    });
    queue.add(["/a/boom.mp3", "/a/fine.mp3"]);
    const summary = await finished;
    expect(summary.imported).toEqual(["ok"]);
    expect(summary.failures).toEqual([{ name: "boom.mp3", message: "exploded" }]);
  });

  it("names unsupported files in the failures without trying them", async () => {
    const importOne = vi.fn(async (): Promise<ImportOutcome> => ({ ok: true, id: "x" }));
    const { queue, finished } = setup(importOne);
    queue.add(["/a/1.mp3"], ["/a/notes.docx"]);
    const summary = await finished;
    expect(importOne).toHaveBeenCalledTimes(1);
    expect(summary.failures).toEqual([{ name: "notes.docx", message: "", unsupported: true }]);
    expect(summary.total).toBe(2);
  });

  it("finishes at once when everything dropped is unsupported", async () => {
    const importOne = vi.fn(async (): Promise<ImportOutcome> => ({ ok: true, id: "x" }));
    const { queue, finished } = setup(importOne);
    queue.add([], ["/a/notes.docx", "/a/photo.png"]);
    const summary = await finished;
    expect(importOne).not.toHaveBeenCalled();
    expect(summary.imported).toEqual([]);
    expect(summary.failures.map((f) => f.name)).toEqual(["notes.docx", "photo.png"]);
  });

  it("reports progress as finished of total, with the current name", async () => {
    const { queue, snapshots, finished } = setup(async () => ({ ok: true, id: "x" }));
    queue.add(["/a/1.mp3", "/a/2.mp3"]);
    await finished;
    const working = snapshots.filter((s) => s.current).map((s) => `${s.finished}/${s.total} ${s.current}`);
    expect(working).toContain("0/2 1.mp3");
    expect(working).toContain("1/2 2.mp3");
    expect(snapshots[snapshots.length - 1]).toMatchObject({ running: false, finished: 2, total: 2, current: undefined });
  });

  it("takes files dropped while it runs into the same run and finishes once", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let n = 0;
    const { queue, finished, onFinish } = setup(async () => {
      if (n++ === 0) await gate;
      return { ok: true, id: `id-${n}` };
    });
    queue.add(["/a/1.mp3"]);
    queue.add(["/a/2.mp3"]);
    expect(queue.snapshot.total).toBe(2);
    release();
    const summary = await finished;
    expect(summary.total).toBe(2);
    expect(summary.imported).toHaveLength(2);
    expect(onFinish).toHaveBeenCalledTimes(1);
  });

  it("skips a file dropped again while it's still waiting or importing", async () => {
    const seen: string[] = [];
    let release: () => void = () => {};
    const gate = new Promise<void>((r) => (release = r));
    const done = vi.fn();
    const q = new ImportQueue({
      importOne: async (path) => {
        seen.push(path);
        await gate;
        return { ok: true, id: path };
      },
      onChange: () => {},
      onFinish: done,
    });
    q.add(["/a.m4a", "/b.m4a"]);
    q.add(["/a.m4a", "/b.m4a", "/c.m4a"]);
    release();
    await vi.waitFor(() => expect(done).toHaveBeenCalledTimes(1));
    expect(seen).toEqual(["/a.m4a", "/b.m4a", "/c.m4a"]);
    expect(done.mock.calls[0][0].total).toBe(3);
  });

  it("starts a fresh run after one has finished", async () => {
    const { queue, finished } = setup(async () => ({ ok: false, message: "no" }));
    queue.add(["/a/1.mp3"]);
    await finished;
    expect(queue.snapshot.failures).toHaveLength(1);
    queue.add(["/a/2.mp3"]);
    expect(queue.snapshot.total).toBe(1);
    expect(queue.snapshot.failures).toHaveLength(0);
  });
});
