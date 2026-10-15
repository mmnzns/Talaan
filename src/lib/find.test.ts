import { describe, expect, it } from "vitest";
import type { TranscriptLine } from "../data/types";
import { findMatches, segments } from "./find";

const line = (id: string, text: string): TranscriptLine => ({ id, at: 0, end: 1000, speaker: "me", text, edited: false });
const lines = [line("a", "Pricing moves to Q1. Pricing is final."), line("b", "No change."), line("c", "the PRICING deck")];

describe("findMatches", () => {
  it("finds every match in transcript order, ignoring case", () => {
    expect(findMatches(lines, "pricing")).toEqual([
      { lineId: "a", start: 0, end: 7 },
      { lineId: "a", start: 21, end: 28 },
      { lineId: "c", start: 4, end: 11 },
    ]);
  });
  it("treats the query as plain text and ignores blanks", () => {
    expect(findMatches([line("x", "costs (Q1) are $5.00")], "(q1)")).toEqual([{ lineId: "x", start: 6, end: 10 }]);
    expect(findMatches([line("x", "costs are $5.00")], "$5.")).toHaveLength(1);
    expect(findMatches(lines, "   ")).toEqual([]);
    expect(findMatches(lines, " final ")).toEqual([{ lineId: "a", start: 32, end: 37 }]);
  });
});

describe("segments", () => {
  it("splits a line into plain and matched parts, numbered across the transcript", () => {
    const text = lines[0].text;
    const ranges = findMatches(lines, "pricing").map((m, i) => ({ ...m, index: i })).filter((m) => m.lineId === "a");
    expect(segments(text, ranges)).toEqual([
      { text: "Pricing", match: 0 },
      { text: " moves to Q1. " },
      { text: "Pricing", match: 1 },
      { text: " is final." },
    ]);
    expect(segments("No change.", [])).toEqual([{ text: "No change." }]);
  });
});
