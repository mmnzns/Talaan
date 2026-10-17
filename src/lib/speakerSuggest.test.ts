import { describe, expect, it } from "vitest";
import type { Meeting, Speaker } from "../data/types";
import { speakerSuggestions } from "./speakerSuggest";

const line = (speaker: Speaker, i: number) => ({ id: `l${i}`, at: i * 1000, end: i * 1000 + 900, speaker, text: "hi", edited: false });

function meeting(attendees: string[] | undefined, speakers: Meeting["speakers"], heard: Speaker[]): Pick<Meeting, "calendarEvent" | "speakers" | "transcript"> {
  return { calendarEvent: attendees && { title: "Sync", attendees }, speakers, transcript: heard.map(line) };
}

describe("speakerSuggestions", () => {
  it("offers the invite's names, without the user", () => {
    const m = meeting(["You", "Miguel Monzones", "Dana Reyes", "Priya Shah"], { me: "Miguel Monzones" }, ["me", "them:1", "them:2"]);
    expect(speakerSuggestions(m, "Miguel Monzones")).toEqual({ names: ["Dana Reyes", "Priya Shah"], probably: undefined });
  });

  it("leaves out Your name in any case and names already given", () => {
    const m = meeting(["miguel monzones", "Dana Reyes", "Priya Shah", "dana reyes"], { me: "Me", "them:1": "Dana Reyes" }, ["me", "them:1", "them:2"]);
    expect(speakerSuggestions(m, "Miguel Monzones").names).toEqual(["Priya Shah"]);
  });

  it("says who Them probably is on a two-person call with one other attendee", () => {
    const m = meeting(["Jordan Lee", "You"], { me: "Me", "them:0": "Them" }, ["me", "them:0"]);
    expect(speakerSuggestions(m, "")).toEqual({ names: ["Jordan Lee"], probably: { speaker: "them:0", name: "Jordan Lee" } });
  });

  it("doesn't guess with more people, more attendees, or once Them has a name", () => {
    expect(speakerSuggestions(meeting(["Jordan Lee", "Ana Cruz"], { me: "Me", "them:0": "Them" }, ["me", "them:0"]), "").probably).toBeUndefined();
    expect(speakerSuggestions(meeting(["Jordan Lee"], { me: "Me" }, ["me", "them:1", "them:2"]), "").probably).toBeUndefined();
    expect(speakerSuggestions(meeting(["Jordan Lee"], { me: "Me", "them:0": "Jordan L." }, ["me", "them:0"]), "").probably).toBeUndefined();
  });

  it("has nothing to offer without calendar attendees", () => {
    expect(speakerSuggestions(meeting(undefined, { me: "Me" }, ["me", "them:0"]), "")).toEqual({ names: [] });
    expect(speakerSuggestions(meeting([], { me: "Me" }, ["me", "them:0"]), "")).toEqual({ names: [] });
  });
});
