// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import { MockSettingsStore } from "../../data/mock/settings";
import { MockMeetingStore } from "../../data/mock/store";
import { seedMeetings } from "../../data/mock/seed";
import type { Meeting, Summary } from "../../data/types";
import { detailedMeeting } from "../../lib/detailedSummary.fixture";
import { ToastProvider } from "../../ui";
import { SummaryCard } from "./SummaryCard";

afterEach(cleanup);

function show(meeting: Meeting, editing = false, onSave?: (s: Summary) => void) {
  const meetings = new MockMeetingStore(new Date("2026-10-14T20:00:00"), { yourName: () => "Me" });
  if (onSave) vi.spyOn(meetings, "updateSummary").mockImplementation(async (_id, s) => void onSave(s));
  render(
    <StoreProvider meetings={meetings} settings={new MockSettingsStore()}>
      <ToastProvider>
        <SummaryCard meeting={meeting} editing={editing} onJump={() => {}} />
      </ToastProvider>
    </StoreProvider>,
  );
}

describe("the detailed summary card", () => {
  it("shows purpose, bold takeaway labels, topics with nested bullets, decisions, next steps by person and open questions in that order", () => {
    show({ ...detailedMeeting(), status: "ready" });
    const labels = ["purpose", "key takeaways", "topics", "decisions", "next steps", "open questions"];
    const text = document.body.textContent ?? "";
    const at = labels.map((l) => text.indexOf(l));
    expect(at.every((i) => i >= 0)).toBe(true);
    expect([...at].sort((a, b) => a - b)).toEqual(at);
    expect(screen.getByText("To plan the pilot of the new scheduling tool.")).toBeTruthy();
    // The takeaway label is bold.
    expect(screen.getByText("Pilot scope").closest("b")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Pilot plan" })).toBeTruthy();
    // A sub-bullet sits in a list inside its bullet's list item.
    const sub = screen.getByText("Support team first");
    expect(sub.closest("li")?.parentElement?.closest("li")?.textContent).toContain("Two teams, about 25 people in total.");
    // The old recap paragraph is not shown next to the purpose.
    expect(screen.queryByText(/Budget: The licence/)).toBeNull();
  });

  it("groups the next steps under each person, owners in order of first appearance, nobody last", () => {
    show({ ...detailedMeeting(), status: "ready" });
    const names = ["Alex", "Rosa Lim", "no owner"];
    const chips = names.map((n) => screen.getByText(n, { selector: "span.rounded-full" }));
    const all = Array.from(document.querySelectorAll("span.rounded-full"));
    const order = chips.map((c) => all.indexOf(c));
    expect([...order].sort((a, b) => a - b)).toEqual(order);
    // The user's two steps (written Alex and alex) share one group.
    const group = chips[0].parentElement!;
    expect(within(group).getByText("Send the rollout plan.")).toBeTruthy();
    expect(within(group).getByText("Share the training slides.")).toBeTruthy();
    // The task rows don't repeat the owner.
    expect(within(group).getAllByText("Alex")).toHaveLength(1);
  });

  it("an older summary shows the recap and a flat list of action items, as before", () => {
    const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;
    show({ ...q4, status: "ready" });
    expect(screen.getByText(q4.summary!.recap)).toBeTruthy();
    expect(screen.getByText("action items")).toBeTruthy();
    expect(screen.queryByText("next steps")).toBeNull();
    expect(screen.queryByText("key takeaways")).toBeNull();
    expect(screen.queryByText("topics")).toBeNull();
  });

  it("editing a takeaway keeps every new field and rewrites the recap from them", () => {
    const saved: Summary[] = [];
    show({ ...detailedMeeting(), status: "ready" }, true, (s) => saved.push(s));
    const box = screen.getAllByRole("textbox").find((b) => b.textContent === "Two teams start in March.")!;
    box.textContent = "Three teams start in March.";
    fireEvent.blur(box);
    expect(saved).toHaveLength(1);
    const s = saved[0];
    expect(s.takeaways![0].text).toBe("Three teams start in March.");
    expect(s.topics).toHaveLength(2);
    expect(s.topics![0].bullets[0].subBullets).toEqual(["Support team first", "Sales team in week 3"]);
    expect(s.purpose).toBe("To plan the pilot of the new scheduling tool.");
    expect(s.recap).toContain("Pilot scope: Three teams start in March.");
    expect(s.edited).toBe(true);
  });

  it("editing a sub-bullet changes only that line", () => {
    const saved: Summary[] = [];
    show({ ...detailedMeeting(), status: "ready" }, true, (s) => saved.push(s));
    const box = screen.getAllByRole("textbox").find((b) => b.textContent === "Sales team in week 3")!;
    box.textContent = "Sales team in week 4";
    fireEvent.blur(box);
    expect(saved[0].topics![0].bullets[0].subBullets).toEqual(["Support team first", "Sales team in week 4"]);
    expect(saved[0].topics![0].bullets[1].text).toBe("Review after six weeks.");
  });
});
