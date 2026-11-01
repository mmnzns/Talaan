// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StoreProvider } from "../../data/StoreProvider";
import type { MeetingStore } from "../../data/store";
import type { SettingsStore } from "../../data/settings";
import { ToastProvider } from "../../ui";
import { DeleteModelLink } from "./DeleteModelLink";

afterEach(cleanup);

function setup(deleteModel: (id: string) => Promise<{ erased: boolean; otherApps: string[] }> = async () => ({ erased: true, otherApps: [] })) {
  const store = { deleteModel: vi.fn(deleteModel) } as unknown as SettingsStore;
  render(
    <StoreProvider meetings={{} as MeetingStore} settings={store}>
      <ToastProvider>
        <DeleteModelLink id="whisper-large-v3-turbo" name="Whisper Large Turbo" sizeMb={1624} />
      </ToastProvider>
    </StoreProvider>,
  );
  return store;
}

describe("DeleteModelLink", () => {
  it("does not delete on the first click: it asks, naming the model and its size", async () => {
    const user = userEvent.setup();
    const store = setup();
    await user.click(screen.getByRole("button", { name: "Delete Whisper Large Turbo" }));
    expect(store.deleteModel).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("Whisper Large Turbo");
    expect(dialog.textContent).toContain("1.6 GB");
    expect(dialog.textContent).toContain("Talaan will stop using it");
    expect(dialog.textContent).toContain("if one does, the file stays");
    expect(dialog.textContent).toContain("download it again");
  });

  it("tells the person when another app keeps the file", async () => {
    const user = userEvent.setup();
    setup(async () => ({ erased: false, otherApps: ["Parlato"] }));
    await user.click(screen.getByRole("button", { name: "Delete Whisper Large Turbo" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText(/Parlato still uses it, so it stays on this computer/)).toBeTruthy();
  });

  it("puts focus on Cancel, and Cancel closes without deleting", async () => {
    const user = userEvent.setup();
    const store = setup();
    const link = screen.getByRole("button", { name: "Delete Whisper Large Turbo" });
    await user.click(link);
    const cancel = screen.getByRole("button", { name: "Cancel" });
    expect(document.activeElement).toBe(cancel);
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(store.deleteModel).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(link);
  });

  it("Escape closes without deleting", async () => {
    const user = userEvent.setup();
    const store = setup();
    await user.click(screen.getByRole("button", { name: "Delete Whisper Large Turbo" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(store.deleteModel).not.toHaveBeenCalled();
  });

  it("deletes only after the confirm button", async () => {
    const user = userEvent.setup();
    const store = setup();
    await user.click(screen.getByRole("button", { name: "Delete Whisper Large Turbo" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(store.deleteModel).toHaveBeenCalledExactlyOnceWith("whisper-large-v3-turbo");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("shows the error and closes when the delete fails", async () => {
    const user = userEvent.setup();
    setup(async () => {
      throw new Error("disk is busy");
    });
    await user.click(screen.getByRole("button", { name: "Delete Whisper Large Turbo" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText(/Something went wrong/)).toBeTruthy();
  });
});
