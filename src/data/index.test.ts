import { describe, expect, it } from "vitest";
import { meetingStore, settingsStore, usingRealStorage } from "./index";
import { MockMeetingStore } from "./mock/store";
import { MockSettingsStore } from "./mock/settings";

describe("store selection", () => {
  it("uses the sample data outside the Tauri app", () => {
    expect(usingRealStorage).toBe(false);
    expect(meetingStore).toBeInstanceOf(MockMeetingStore);
    expect(settingsStore).toBeInstanceOf(MockSettingsStore);
  });
});
