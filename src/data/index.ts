import { MockMeetingStore } from "./mock/store";
import { MockSettingsStore } from "./mock/settings";
import type { MeetingStore } from "./store";
import type { SettingsStore } from "./settings";

/** `?mock=empty` in the URL starts with no meetings, to check the empty states.
 *  The only place the mock is wired in. M2 replaces these two lines with the real stores. */
export const meetingStore: MeetingStore = new MockMeetingStore(new Date(), { empty: typeof location !== "undefined" && location.search.includes("mock=empty") });
export const settingsStore: SettingsStore = new MockSettingsStore();
