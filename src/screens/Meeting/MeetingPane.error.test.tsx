// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import type { SettingsStore } from "../../data/settings";
import type { MeetingStore } from "../../data/store";
import { MeetingPane } from "./MeetingPane";

afterEach(cleanup);

describe("MeetingPane when the meeting can't be read", () => {
  it("says what happened and offers Try again and Show files, not a blank pane", async () => {
    const user = userEvent.setup();
    const getMeeting = vi.fn().mockRejectedValue({ code: "unsupported_schema", message: "newer" });
    const revealMeeting = vi.fn().mockResolvedValue(undefined);
    const meetings = { getMeeting, revealMeeting, subscribe: () => () => {} } as unknown as MeetingStore;
    const settings = { subscribe: () => () => {} } as unknown as SettingsStore;
    render(
      <StoreProvider meetings={meetings} settings={settings}>
        <MeetingPane id="m1" />
      </StoreProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Can’t open this meeting");
    expect(alert.textContent).toContain("Its files are still on this computer.");
    await user.click(screen.getByRole("button", { name: "Show files on this computer" }));
    expect(revealMeeting).toHaveBeenCalledWith("m1");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(getMeeting).toHaveBeenCalledTimes(2));
  });
});
