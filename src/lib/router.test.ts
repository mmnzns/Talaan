import { describe, expect, it } from "vitest";
import { parseRoute, routes } from "./router";

describe("parseRoute", () => {
  it("defaults to the meeting list", () => {
    expect(parseRoute("")).toEqual({ name: "meetings" });
    expect(parseRoute("#/")).toEqual({ name: "meetings" });
    expect(parseRoute("#/nonsense")).toEqual({ name: "meetings" });
  });
  it("parses a meeting with a jump time", () => {
    expect(parseRoute("#/meeting/m-q4?t=33000")).toEqual({ name: "meeting", id: "m-q4", t: 33000 });
    expect(parseRoute("#/meeting/m-q4")).toEqual({ name: "meeting", id: "m-q4", t: undefined });
  });
  it("parses ask, screens and settings", () => {
    expect(parseRoute("#/ask?m=m-q4")).toEqual({ name: "ask", meetingId: "m-q4" });
    expect(parseRoute("#/screens/recording")).toEqual({ name: "screens", which: "recording" });
    expect(parseRoute("#/settings/privacy")).toEqual({ name: "settings", tab: "privacy" });
    expect(parseRoute("#/settings")).toEqual({ name: "settings", tab: "general" });
  });
  it("round-trips the route builders", () => {
    expect(parseRoute(routes.meeting("m-q4", 5000))).toEqual({ name: "meeting", id: "m-q4", t: 5000 });
  });
});

describe("print route", () => {
  it("round-trips the meeting and the transcript choice", () => {
    expect(parseRoute(routes.print("2026-10-14_1431_yv57", true))).toEqual({ name: "print", id: "2026-10-14_1431_yv57", withTranscript: true });
    expect(parseRoute(routes.print("m1", false))).toEqual({ name: "print", id: "m1", withTranscript: false });
  });
});
