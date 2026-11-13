// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import { MockSettingsStore } from "../../data/mock/settings";
import { MockMeetingStore } from "../../data/mock/store";
import { seedMeetings } from "../../data/mock/seed";
import type { Meeting } from "../../data/types";
import { ToastProvider } from "../../ui";
import { StatusBanner } from "./StatusBanner";

afterEach(cleanup);

const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;

function show(meeting: Meeting) {
  const meetings = new MockMeetingStore(new Date("2026-10-14T20:00:00"), { yourName: () => "Me" });
  render(
    <StoreProvider meetings={meetings} settings={new MockSettingsStore()}>
      <ToastProvider>
        <StatusBanner meeting={meeting} />
      </ToastProvider>
    </StoreProvider>,
  );
}

describe("a recording that stopped saving", () => {
  it("says when the audio stopped and what the transcript covers", () => {
    show({ ...q4, status: "ready", recordingIssue: { kind: "write_failed", atMs: 1_385_000 } });
    expect(screen.getByText("Part of the recording wasn’t saved")).toBeTruthy();
    expect(
      screen.getByText("Audio stopped saving at 23:05 because the disk was full or couldn’t be written to. The transcript covers what was saved."),
    ).toBeTruthy();
  });

  it("is also shown while the saved part is being transcribed", () => {
    show({ ...q4, status: "transcribing", recordingIssue: { kind: "write_failed", atMs: 3_725_000 } });
    expect(screen.getByText(/Audio stopped saving at 1:02:05/)).toBeTruthy();
  });

  it("a meeting where nothing could be saved says so instead of offering to transcribe again", () => {
    show({
      ...q4,
      status: "failed",
      hasAudio: false,
      error: { code: "write_failed", message: "Nothing could be saved because the disk was full or couldn't be written to." },
    });
    expect(screen.getByText("Nothing was saved from this recording")).toBeTruthy();
    expect(screen.getByText(/disk was full or couldn’t be written to, so no audio was saved/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(screen.getByRole("button", { name: "Delete" })).toBeTruthy();
    expect(screen.queryByText("Couldn’t write the transcript")).toBeNull();
  });

  it("a normal meeting has no such banner", () => {
    show({ ...q4, status: "ready" });
    expect(screen.queryByText("Part of the recording wasn’t saved")).toBeNull();
  });
});
