import { describe, expect, it } from "vitest";
import type { MeetingListItem } from "../../data/types";
import { rowLabel, statusLine, unreadable } from "./MeetingItem";

const item = (patch: Partial<MeetingListItem> = {}): MeetingListItem => ({
  id: "m1",
  title: "Weekly 1:1",
  app: "slack",
  appLabel: "Slack huddle",
  startedAt: "2026-10-15T16:00:00Z",
  durationMs: 31 * 60_000,
  status: "ready",
  tags: [],
  peopleCount: 2,
  pinned: false,
  ...patch,
});

// What Rust lists for a meeting.json it can't read (storage/store.rs broken_item).
const broken = item({ id: "2026-10-01_0900_ab12", title: "2026-10-01_0900_ab12", app: "other", appLabel: "Unreadable", startedAt: "", durationMs: 0, status: "failed" });

describe("meeting list rows", () => {
  it("say the title, the date and the line under it", () => {
    const label = rowLabel(item({ pinned: true }));
    expect(label).toMatch(/^Weekly 1:1, Oct 1[56], Slack huddle · .* · 31 min, Audio kept forever$/);
  });

  it("give an unreadable meeting its own words and no Invalid Date", () => {
    expect(unreadable(broken)).toBe(true);
    expect(unreadable(item({ status: "failed" }))).toBe(false);
    expect(statusLine(broken)).toBe("Can’t be opened: damaged, or made by a newer Talaan");
    expect(rowLabel(broken)).toBe("2026-10-01_0900_ab12, Can’t be opened: damaged, or made by a newer Talaan");
    expect(rowLabel(broken)).not.toContain("Invalid");
    expect(statusLine(item({ status: "failed" }))).toBe("Couldn’t write the transcript");
  });
});
