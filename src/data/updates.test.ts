import { beforeEach, describe, expect, it } from "vitest";
import { StoreError } from "./errors";
import type { SettingsStore } from "./settings";
import { bannerVisible, found, install, later, progressed, resetUpdateForTests, showBanner, updateSnapshot } from "./updates";

const store = (installUpdate: () => Promise<void>) => ({ installUpdate }) as unknown as SettingsStore;

describe("the pending update", () => {
  beforeEach(() => resetUpdateForTests());

  it("shows the banner when a check finds a version", () => {
    expect(bannerVisible(updateSnapshot())).toBe(false);
    found({ version: "1.2.0" });
    expect(bannerVisible(updateSnapshot())).toBe(true);
  });

  it("Later hides it for that version only, and a newer one shows again", () => {
    found({ version: "1.2.0" });
    later();
    expect(bannerVisible(updateSnapshot())).toBe(false);
    expect(updateSnapshot().pending?.version).toBe("1.2.0"); // the chip and the menu bar line still know
    found({ version: "1.2.0" }); // the next 6-hour check
    expect(bannerVisible(updateSnapshot())).toBe(false);
    found({ version: "1.3.0" });
    expect(bannerVisible(updateSnapshot())).toBe(true);
  });

  it("the chip (or a notification click) brings it back", () => {
    found({ version: "1.2.0" });
    later();
    showBanner();
    expect(bannerVisible(updateSnapshot())).toBe(true);
  });

  it("Update now shows progress, then Restarting", async () => {
    found({ version: "1.2.0" });
    let finish!: () => void;
    const running = install(store(() => new Promise<void>((r) => (finish = r))));
    expect(updateSnapshot().phase).toEqual({ state: "installing", percent: null });
    finish();
    await running;
    expect(updateSnapshot().phase).toEqual({ state: "restarting" });
  });

  it("a refusal (recording) or failure is shown in plain words, and Later clears it", async () => {
    found({ version: "1.2.0" });
    await install(store(() => Promise.reject(new StoreError("refused", "Stop the recording first, then update."))));
    expect(updateSnapshot().phase).toEqual({ state: "error", message: "Stop the recording first, then update." });
    expect(bannerVisible(updateSnapshot())).toBe(true);
    later();
    expect(updateSnapshot().phase).toEqual({ state: "idle" });
    expect(bannerVisible(updateSnapshot())).toBe(false);
  });

  it("a window that only heard the progress also hears the failure", () => {
    found({ version: "1.2.0" });
    progressed({ percent: 40 }); // another window clicked Update now
    expect(updateSnapshot().phase).toEqual({ state: "installing", percent: 40 });
    progressed({ error: "The update didn't install: signature mismatch" });
    expect(updateSnapshot().phase).toEqual({ state: "error", message: "The update didn't install: signature mismatch" });
    progressed({ percent: 0 }); // tried again from somewhere
    expect(updateSnapshot().phase).toEqual({ state: "installing", percent: 0 });
    progressed({ percent: 100, restarting: true });
    expect(updateSnapshot().phase).toEqual({ state: "restarting" });
  });

  it("a second click while installing does nothing", async () => {
    found({ version: "1.2.0" });
    let calls = 0;
    const s = store(() => {
      calls++;
      return new Promise<void>(() => {});
    });
    void install(s);
    void install(s);
    expect(calls).toBe(1);
  });
});
