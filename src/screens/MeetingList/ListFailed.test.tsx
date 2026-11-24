// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import type { SettingsStore } from "../../data/settings";
import type { MeetingStore } from "../../data/store";
import { MeetingList } from "./MeetingList";

afterEach(cleanup);

describe("MeetingList when the list can't be read", () => {
  it("shows an error with Try again and Open folder instead of the empty-list text", async () => {
    const user = userEvent.setup();
    const listMeetings = vi.fn().mockRejectedValue(new Error("ipc"));
    const revealData = vi.fn().mockResolvedValue(undefined);
    const meetings = {
      listMeetings,
      listFolders: async () => [],
      listTags: async () => [],
      subscribe: () => () => {},
    } as unknown as MeetingStore;
    const settings = { revealData, subscribe: () => () => {} } as unknown as SettingsStore;
    render(
      <StoreProvider meetings={meetings} settings={settings}>
        <MeetingList />
      </StoreProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Couldn’t load your meetings");
    expect(screen.queryByText("No meetings yet")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Open folder" }));
    expect(revealData).toHaveBeenCalled();
    const calls = listMeetings.mock.calls.length;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(listMeetings.mock.calls.length).toBeGreaterThan(calls);
  });
});
