// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "../../data/defaultSettings";
import type { Settings, SettingsStore } from "../../data/settings";
import { StoreProvider } from "../../data/StoreProvider";
import type { MeetingStore } from "../../data/store";
import { ToastProvider } from "../../ui";
import { PrivacyTab } from "./PrivacyTab";

afterEach(cleanup);

function setup(preview: { audio: number; meetings: number }) {
  const settings: Settings = { ...defaultSettings, retention: "7d", transcriptRetention: "forever" };
  const update = vi.fn(async (patch: Partial<Settings>) => ({ ...settings, ...patch }));
  const settingsStore = {
    dataInfo: async () => ({ path: "/x", bytes: 1 }),
    retentionPreview: vi.fn(async () => preview),
  } as unknown as SettingsStore;
  const meetings = { listMeetings: async () => [], subscribe: () => () => {} } as unknown as MeetingStore;
  render(
    <StoreProvider meetings={meetings} settings={settingsStore}>
      <ToastProvider>
        <PrivacyTab settings={settings} update={update} />
      </ToastProvider>
    </StoreProvider>,
  );
  return { update, settingsStore };
}

describe("PrivacyTab retention choices", () => {
  it("asks before a stricter transcript limit deletes meetings, with Cancel focused", async () => {
    const user = userEvent.setup();
    const { update, settingsStore } = setup({ audio: 0, meetings: 3 });
    await user.selectOptions(screen.getByLabelText("Keep transcripts"), "90d");
    const dialog = await screen.findByRole("dialog");
    expect(settingsStore.retentionPreview).toHaveBeenCalledWith("7d", "90d");
    expect(dialog.textContent).toContain("Delete 3 meetings now?");
    expect(dialog.textContent).toContain("Meetings older than 90 days, with their transcripts and summaries, are deleted from this computer. Pinned meetings are kept. This can’t be undone.");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
    expect(update).not.toHaveBeenCalled();
  });

  it("Cancel leaves the setting unchanged", async () => {
    const user = userEvent.setup();
    const { update } = setup({ audio: 0, meetings: 3 });
    const select = screen.getByLabelText("Keep transcripts") as HTMLSelectElement;
    await user.selectOptions(select, "90d");
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(update).not.toHaveBeenCalled();
    expect(select.value).toBe("forever");
  });

  it("saves the new limit once the deletion is confirmed", async () => {
    const user = userEvent.setup();
    const { update } = setup({ audio: 0, meetings: 3 });
    await user.selectOptions(screen.getByLabelText("Keep transcripts"), "90d");
    await user.click(await screen.findByRole("button", { name: "Delete 3 meetings" }));
    await waitFor(() => expect(update).toHaveBeenCalledWith({ transcriptRetention: "90d" }));
  });

  it("saves without asking when nothing would be deleted", async () => {
    const user = userEvent.setup();
    const { update } = setup({ audio: 0, meetings: 0 });
    await user.selectOptions(screen.getByLabelText("Keep transcripts"), "90d");
    await waitFor(() => expect(update).toHaveBeenCalledWith({ transcriptRetention: "90d" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks with the number of meetings that would lose their audio", async () => {
    const user = userEvent.setup();
    const { update, settingsStore } = setup({ audio: 2, meetings: 0 });
    await user.selectOptions(screen.getByLabelText("Keep audio for"), "1d");
    const dialog = await screen.findByRole("dialog");
    expect(settingsStore.retentionPreview).toHaveBeenCalledWith("1d", "forever");
    expect(dialog.textContent).toContain("Delete the audio of 2 meetings now?");
    expect(dialog.textContent).toContain("Audio older than 1 day is deleted");
    expect(update).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Delete audio" }));
    await waitFor(() => expect(update).toHaveBeenCalledWith({ retention: "1d" }));
  });

  it("says what Don't keep does", async () => {
    const user = userEvent.setup();
    setup({ audio: 4, meetings: 0 });
    await user.selectOptions(screen.getByLabelText("Keep audio for"), "none");
    expect((await screen.findByRole("dialog")).textContent).toContain("every meeting except pinned ones and those that failed");
  });
});
