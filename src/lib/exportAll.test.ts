import { describe, expect, it } from "vitest";
import type { Meeting } from "../data/types";
import { exportAllFiles } from "./exportAll";

const meeting = (id: string, title: string, startedAt: string, status: Meeting["status"] = "ready") =>
  ({
    id, title, startedAt, status, app: "zoom", appLabel: "Zoom", durationMs: 60_000, tags: [], peopleCount: 2, pinned: false,
    language: "en", hasAudio: false, speakers: { me: "Miguel" }, marks: [], pauses: [],
    transcript: [{ id: "l0", at: 0, end: 1000, speaker: "me", text: "Send the deck.", edited: false }],
  }) as Meeting;

describe("exportAllFiles", () => {
  it("names files by date and title and keeps names unique", () => {
    const files = exportAllFiles([
      meeting("a", "Q4 sync", "2026-10-14T14:30:00-07:00"),
      meeting("b", "Q4 sync", "2026-10-14T16:00:00-07:00"),
      meeting("c", "a/b: c?", "2026-10-13T09:00:00-07:00"),
    ]);
    expect(files.map((f) => f.name)).toEqual(["2026-10-14 Q4 sync.md", "2026-10-14 Q4 sync (2).md", "2026-10-13 ab c.md"]);
  });
  it("includes the transcript with speaker names and skips a recording in progress", () => {
    const files = exportAllFiles([meeting("a", "Sync", "2026-10-14T14:30:00-07:00"), meeting("b", "Live", "2026-10-14T15:00:00-07:00", "recording")]);
    expect(files).toHaveLength(1);
    expect(files[0].text).toContain("Miguel:** Send the deck.");
  });
});
