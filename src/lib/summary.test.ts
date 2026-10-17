import { describe, expect, it } from "vitest";
import { sampleSummary } from "../data/mock/seed";
import { summaryHasChanges } from "./summary";

describe("summaryHasChanges", () => {
  it("is true when the summary was edited or an action item ticked", () => {
    const s = sampleSummary();
    expect(summaryHasChanges(undefined)).toBe(false);
    expect(summaryHasChanges(s)).toBe(false);
    expect(summaryHasChanges({ ...s, edited: true })).toBe(true);
    expect(summaryHasChanges({ ...s, actionItems: s.actionItems.map((a) => ({ ...a, done: true })) })).toBe(true);
  });
});
