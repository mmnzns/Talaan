import { inTauri } from "../lib/windows";
import { MockMeetingStore } from "./mock/store";
import { MockSettingsStore } from "./mock/settings";
import { TauriMeetingStore } from "./tauri/meetingStore";
import { TauriSettingsStore } from "./tauri/settingsStore";
import type { MeetingStore } from "./store";
import type { SettingsStore } from "./settings";

/**
 * The only place the stores are chosen. In the Tauri app the data comes from disk; in a plain browser
 * (`npm run dev`) it is sample data, which keeps screenshots and quick UI work running without the backend.
 * `?mock=empty` in the URL starts the sample data with no meetings.
 */
const real = inTauri();
const empty = typeof location !== "undefined" && location.search.includes("mock=empty");

export const meetingStore: MeetingStore = real ? new TauriMeetingStore() : new MockMeetingStore(new Date(), { empty });
export const settingsStore: SettingsStore = real ? new TauriSettingsStore() : new MockSettingsStore();
export const usingRealStorage = real;
