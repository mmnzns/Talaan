import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { MeetingListItem } from "../../data/types";
import { MeetingDays } from "./MeetingList";

const item = (id: string, startedAt: string): MeetingListItem => ({
  id,
  title: id,
  app: "slack",
  appLabel: "Slack huddle",
  startedAt,
  durationMs: 30 * 60_000,
  status: "ready",
  tags: [],
  peopleCount: 2,
  pinned: false,
});

describe("meeting list days", () => {
  it("show a date on every row, including the second meeting of a day", () => {
    const a = item("a", "2026-10-15T18:00:00");
    const b = item("b", "2026-10-15T14:00:00");
    const html = renderToStaticMarkup(<MeetingDays days={[{ key: "oct 15", label: "oct 15", items: [a, b] }]} />);
    expect(html.match(/font-display[^>]*>15</g)).toHaveLength(2);
  });
});
