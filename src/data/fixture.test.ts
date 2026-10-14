import { describe, expect, it } from "vitest";
import sample from "../../fixtures/meeting.sample.json";
import { MockMeetingStore } from "./mock/store";

const STATUSES = ["recording", "transcribing", "summarizing", "ready", "failed", "needs_recovery"];

/** The Rust side round-trips this same file (storage/model.rs). If either type drifts, one of the tests fails. */
describe("fixtures/meeting.sample.json", () => {
  it("has exactly the fields of a Meeting from the store", async () => {
    const store = new MockMeetingStore(new Date("2026-10-14T20:00:00"), { askDelayMs: 0 });
    const meeting = JSON.parse(JSON.stringify(await store.getMeeting("m-q4")));
    expect(Object.keys(sample).sort()).toEqual(Object.keys(meeting).sort());
  });

  it("uses valid status and speaker values", () => {
    expect(STATUSES).toContain(sample.status);
    for (const l of sample.transcript) expect(l.speaker).toMatch(/^(me|them:\d+)$/);
    expect(Object.keys(sample.speakers).every((k) => /^(me|them:\d+)$/.test(k))).toBe(true);
  });
});
