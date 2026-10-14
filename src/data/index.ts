import { MockMeetingStore } from "./mock/store";
import { MockSettingsStore } from "./mock/settings";
import type { MeetingStore } from "./store";
import type { SettingsStore } from "./settings";

/** The only place the mock is wired in. M2 replaces these two lines with the real stores. */
export const meetingStore: MeetingStore = new MockMeetingStore();
export const settingsStore: SettingsStore = new MockSettingsStore();
