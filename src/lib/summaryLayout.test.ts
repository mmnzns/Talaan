import { describe, expect, it } from "vitest";
import { detailedMeeting } from "./detailedSummary.fixture";
import { isDetailed, nextStepsByOwner, recapFrom } from "./summaryLayout";
import { seedMeetings } from "../data/mock/seed";

describe("the detailed layout", () => {
  it("is on for a summary with a purpose, takeaways or topics, and off for an older one", () => {
    expect(isDetailed(detailedMeeting().summary!)).toBe(true);
    expect(isDetailed({ topics: [{ id: "t", title: "x", bullets: [] }] })).toBe(true);
    const old = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!.summary!;
    expect(isDetailed(old)).toBe(false);
    expect(isDetailed({ purpose: "  " })).toBe(false);
  });

  it("groups next steps by person: owners in the order they first appear, nobody last", () => {
    const m = detailedMeeting();
    const groups = nextStepsByOwner(m, m.summary!.actionItems);
    expect(groups.map((g) => g.owner)).toEqual(["Alex", "Rosa Lim", undefined]);
    // "alex" in another case is the same person; items keep their order inside the group.
    expect(groups[0].items.map((a) => a.id)).toEqual(["a0", "a3"]);
    expect(groups[2].items.map((a) => a.id)).toEqual(["a2"]);
  });

  it("puts the user's steps together whether the summary wrote Me or the name", () => {
    const m = detailedMeeting();
    const items = [
      { id: "x0", text: "One.", owner: "Me", done: false },
      { id: "x1", text: "Two.", owner: "Dana", done: false },
      { id: "x2", text: "Three.", owner: "Alex", done: false },
    ];
    const groups = nextStepsByOwner(m, items);
    expect(groups.map((g) => [g.owner, g.items.length])).toEqual([["Alex", 2], ["Dana", 1]]);
  });

  it("writes the recap as plain sentences from the purpose and the takeaways", () => {
    const s = detailedMeeting().summary!;
    expect(recapFrom(s.purpose, s.takeaways)).toBe("To plan the pilot of the new scheduling tool. Pilot scope: Two teams start in March. Budget: The licence fits the current budget.");
    expect(recapFrom(undefined, undefined)).toBe("");
  });
});
