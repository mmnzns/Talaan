import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import { detailedMeeting } from "./detailedSummary.fixture";
import { isDetailed, nextStepsByOwner, recapFrom, taskText } from "./summaryLayout";
import { seedMeetings } from "../data/mock/seed";

describe("the detailed layout", () => {
  afterEach(() => setLang("en"));

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

  it("puts the user's group first, then the others in order, nobody last", () => {
    const m = { ...detailedMeeting(), speakers: { me: "Alex" } as never };
    const item = (id: string, owner?: string) => ({ id, text: id, owner, done: false });
    const items = [item("1", "Rosa"), item("2"), item("3", "Dana"), item("4", "me"), item("5", "Alex")];
    expect(nextStepsByOwner(m, items).map((g) => [g.owner, g.items.map((a) => a.id)])).toEqual([["Alex", ["4", "5"]], ["Rosa", ["1"]], ["Dana", ["3"]], [undefined, ["2"]]]);
  });

  it("calls the user's group You while Your name is empty", () => {
    const m = { ...detailedMeeting(), speakers: {} as never };
    const item = (id: string, owner?: string) => ({ id, text: id, owner, done: false });
    const groups = nextStepsByOwner(m, [item("1", "Rosa"), item("2", "Me")]);
    expect(groups.map((g) => g.owner)).toEqual(["You", "Rosa"]);
  });

  it("calls the user's group by the UI language's You, whatever the summary language wrote", () => {
    const m = { ...detailedMeeting(), speakers: {} as never };
    const item = (id: string, owner?: string) => ({ id, text: id, owner, done: false });
    // The model writes the token Me; a summary from an earlier build wrote You; both are the same person.
    setLang("fr");
    expect(nextStepsByOwner(m, [item("1", "Rosa"), item("2", "Me"), item("3", "You")]).map((g) => [g.owner, g.items.length])).toEqual([["Vous", 2], ["Rosa", 1]]);
    setLang("es");
    expect(nextStepsByOwner(m, [item("1", "Me")]).map((g) => g.owner)).toEqual(["Tú"]);
    // Your name wins over the label, for Me and You alike.
    setLang("fr");
    const named = { ...detailedMeeting(), speakers: { me: "Alex" } as never };
    expect(nextStepsByOwner(named, [item("1", "You"), item("2", "Me")]).map((g) => [g.owner, g.items.length])).toEqual([["Alex", 2]]);
  });

  it("trims a trailing period before a due date or time goes after the task", () => {
    expect(taskText({ text: "Confirm the estimate with engineering. ", due: "Thursday" }, true)).toBe("Confirm the estimate with engineering");
    expect(taskText({ text: "Send it.", at: 5000 }, true)).toBe("Send it");
    expect(taskText({ text: "Send it." }, true)).toBe("Send it.");
    expect(taskText({ text: "...", due: "Fri" }, true)).toBe("...");
    // A summary that is not detailed keeps the text exactly as written.
    expect(taskText({ text: "Send it. ", due: "Fri", at: 5000 }, false)).toBe("Send it. ");
  });

  it("writes the recap as plain sentences from the purpose and the takeaways", () => {
    const s = detailedMeeting().summary!;
    expect(recapFrom(s.purpose, s.takeaways)).toBe("To plan the pilot of the new scheduling tool. Pilot scope: Two teams start in March. Budget: The licence fits the current budget.");
    expect(recapFrom(undefined, undefined)).toBe("");
  });
});
