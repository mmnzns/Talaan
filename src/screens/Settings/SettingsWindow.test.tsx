// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockSettingsStore } from "../../data/mock/settings";
import { StoreProvider } from "../../data/StoreProvider";
import type { MeetingStore } from "../../data/store";
import { ToastProvider } from "../../ui";
import { SettingsWindow } from "./SettingsWindow";

afterEach(cleanup);

describe("SettingsWindow when saving fails", () => {
  it("shows an error, keeps the change visible, and Try again saves it", async () => {
    const user = userEvent.setup();
    const settings = new MockSettingsStore();
    const real = settings.update.bind(settings);
    const update = vi.spyOn(settings, "update").mockRejectedValueOnce({ code: "refused", message: "The disk is full." }).mockImplementation(real);
    const meetings = { listMeetings: async () => [], subscribe: () => () => {} } as unknown as MeetingStore;
    render(
      <StoreProvider meetings={meetings} settings={settings}>
        <ToastProvider>
          <SettingsWindow tab="general" />
        </ToastProvider>
      </StoreProvider>,
    );
    const toggle = (await screen.findByRole("switch", { name: "Notify when a transcript or summary is ready" })) as HTMLInputElement;
    const before = toggle.checked;
    await user.click(toggle);
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Couldn’t save your settings");
    expect(alert.textContent).toContain("The disk is full.");
    expect((screen.getByRole("switch", { name: "Notify when a transcript or summary is ready" }) as HTMLInputElement).checked).toBe(!before);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(update).toHaveBeenCalledTimes(2);
  });
});
