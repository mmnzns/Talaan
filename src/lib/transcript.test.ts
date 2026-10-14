import { describe, expect, it } from "vitest";
import type { TranscriptLine } from "../data/types";
import { buildRows, lineAt } from "./transcript";

const line = (id: string, at: number): TranscriptLine => ({ id, at, end: at + 5000, speaker: "me", text: id, edited: false });
const lines = [line("a", 0), line("b", 10_000), line("c", 30_000)];

describe("lineAt", () => {
  it("returns the last line starting at or before the time", () => {
    expect(lineAt(lines, 0)?.id).toBe("a");
    expect(lineAt(lines, 12_000)?.id).toBe("b");
    expect(lineAt(lines, 99_000)?.id).toBe("c");
  });
  it("falls back to the first line and handles empty input", () => {
    expect(lineAt([line("x", 5000)], 0)?.id).toBe("x");
    expect(lineAt([], 0)).toBeUndefined();
  });
});

describe("buildRows", () => {
  it("places a pause between the lines around it", () => {
    const rows = buildRows(lines, [{ at: 20_000, durationMs: 120_000 }], []);
    expect(rows.map((r) => (r.kind === "line" ? r.line.id : "pause"))).toEqual(["a", "b", "pause", "c"]);
  });
  it("keeps a trailing pause at the end", () => {
    const rows = buildRows(lines, [{ at: 60_000, durationMs: 1000 }], []);
    expect(rows[rows.length - 1].kind).toBe("pause");
  });
  it("attaches marks to the line they fall in", () => {
    const rows = buildRows(lines, [], [{ id: "m", at: 12_000, note: "n" }]);
    const b = rows.find((r) => r.kind === "line" && r.line.id === "b");
    expect(b && b.kind === "line" && b.marks.map((m) => m.id)).toEqual(["m"]);
  });
  it("sorts unsorted input by time", () => {
    const rows = buildRows([lines[2], lines[0]], [], []);
    expect(rows.map((r) => (r.kind === "line" ? r.line.id : ""))).toEqual(["a", "c"]);
  });
});

describe("speakerName", () => {
  it("prefers the name the user gave, then Me / Them / Speaker N", async () => {
    const { speakerName } = await import("./transcript");
    const m = { speakers: { "them:1": "Dana Reyes" } };
    expect(speakerName(m, "them:1")).toBe("Dana Reyes");
    expect(speakerName(m, "me")).toBe("Me");
    expect(speakerName(m, "them:0")).toBe("Them");
    expect(speakerName(m, "them:2")).toBe("Speaker 2");
  });
});
