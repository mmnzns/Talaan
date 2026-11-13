// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import { StoreError } from "../../data/errors";
import { MockSettingsStore } from "../../data/mock/settings";
import { MockMeetingStore } from "../../data/mock/store";
import { seedMeetings } from "../../data/mock/seed";
import type { Meeting } from "../../data/types";
import { ToastProvider } from "../../ui";
import { MeetingHeader } from "./MeetingHeader";
import { StatusBanner } from "./StatusBanner";

afterEach(cleanup);

const q4 = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;
const refused = () => Promise.reject(new StoreError("refused", "Stop the recording before deleting this meeting."));

function setup(node: React.ReactNode) {
  const meetings = new MockMeetingStore(new Date("2026-10-14T20:00:00"), { yourName: () => "Me" });
  meetings.deleteMeeting = vi.fn(refused);
  render(
    <StoreProvider meetings={meetings} settings={new MockSettingsStore()}>
      <ToastProvider>{node}</ToastProvider>
    </StoreProvider>,
  );
  return meetings;
}

const header = (meeting: Meeting) => (
  <MeetingHeader meeting={meeting} editing={false} canRevert={false} onToggleEdit={() => {}} onRevert={() => {}} onExport={() => {}} onEmail={() => {}} onCopyEmail={() => {}} />
);

describe("deleting a meeting that can't be deleted", () => {
  it("the header shows why instead of failing silently", async () => {
    const user = userEvent.setup();
    const meetings = setup(header({ ...q4, status: "ready" }));
    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByText("Delete meeting…"));
    await user.click(screen.getByRole("button", { name: "Delete meeting" }));
    expect(meetings.deleteMeeting).toHaveBeenCalled();
    expect(await screen.findByText("Stop the recording before deleting this meeting.")).toBeTruthy();
  });

  it("the interrupted-recording banner shows why instead of failing silently", async () => {
    const user = userEvent.setup();
    const meetings = setup(<StatusBanner meeting={{ ...q4, status: "needs_recovery" }} />);
    await user.click(screen.getByRole("button", { name: "Discard" }));
    expect(meetings.deleteMeeting).toHaveBeenCalled();
    expect(await screen.findByText("Stop the recording before deleting this meeting.")).toBeTruthy();
  });

  it("the meeting being recorded has no Delete to press", () => {
    setup(header({ ...q4, status: "recording" }));
    expect(screen.queryByRole("button", { name: "More actions" })).toBeNull();
  });
});
