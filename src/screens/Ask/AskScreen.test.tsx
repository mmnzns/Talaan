// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import type { MeetingStore } from "../../data/store";
import type { SettingsStore } from "../../data/settings";
import { AskScreen } from "./AskScreen";

afterEach(cleanup);

// jsdom has no scrollIntoView, which the screen calls when turns change.
Element.prototype.scrollIntoView = () => {};

// Only what the screen reads: tags, folders, one meeting and the settings.
function setup(tags: string[], meetingId?: string) {
  const meetings = {
    listTags: async () => tags,
    listFolders: async () => [],
    getMeeting: async () => undefined,
    subscribe: () => () => {},
  } as unknown as MeetingStore;
  const settings = { get: async () => ({ summaryEngine: "local" }), subscribe: () => () => {} } as unknown as SettingsStore;
  render(
    <StoreProvider meetings={meetings} settings={settings}>
      <AskScreen meetingId={meetingId} />
    </StoreProvider>,
  );
}

const chips = () => screen.getAllByRole("button").map((b) => b.textContent ?? "");

describe("Ask suggestions", () => {
  it("offer no tag question when the user has no tags", async () => {
    setup([]);
    await screen.findByText("What did we decide this week?");
    expect(chips()).toEqual(expect.arrayContaining(["What are my open action items?", "What did I promise to do?"]));
    expect(chips().some((c) => c.includes("tagged"))).toBe(false);
  });

  it("name the first tag when the user has one", async () => {
    setup(["budget", "roadmap"]);
    expect(await screen.findByText("Summarize everything tagged #budget")).toBeTruthy();
    expect(chips().some((c) => c.includes("#roadmap"))).toBe(false);
  });

  it("are the single-meeting set when scoped to one meeting, with no tag or week question", async () => {
    setup(["budget"], "m1");
    await screen.findByText("What are the action items?");
    expect(chips()).toEqual(expect.arrayContaining(["What did we decide?", "What are the action items?", "What did I promise to do?"]));
    expect(chips().some((c) => c.includes("tagged") || c.includes("this week"))).toBe(false);
  });
});
