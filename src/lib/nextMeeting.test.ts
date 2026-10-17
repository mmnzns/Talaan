import { describe, expect, it } from "vitest";
import type { NextMeeting } from "../data/settings";
import { appOf, eventOf, startsIn } from "./nextMeeting";

const MIN = 60_000;
const next: NextMeeting = {
  title: "Design review",
  startMs: 100 * MIN,
  endMs: 130 * MIN,
  attendees: ["Ana Reyes"],
  emails: ["ana@example.com"],
  joinUrl: "https://us02web.zoom.us/j/1",
  appId: "zoom",
  appLabel: "Zoom",
};

describe("next meeting", () => {
  it("counts down in whole minutes, then says now, then ends", () => {
    expect(startsIn(next, 88 * MIN)).toBe(12);
    expect(startsIn(next, 88 * MIN + 1)).toBe(12);
    expect(startsIn(next, 100 * MIN - 1000)).toBe(1);
    expect(startsIn(next, 100 * MIN)).toBe("now");
    expect(startsIn(next, 129 * MIN)).toBe("now");
    expect(startsIn(next, 130 * MIN)).toBe("ended");
  });

  it("gives the recording the event and the call app", () => {
    expect(eventOf(next)).toEqual({ title: "Design review", attendees: ["Ana Reyes"], emails: ["ana@example.com"] });
    expect(appOf(next)).toEqual({ id: "zoom", label: "Zoom" });
    expect(appOf({ ...next, appId: null, appLabel: null })).toBeUndefined();
  });
});
