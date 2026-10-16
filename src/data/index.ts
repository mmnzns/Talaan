import { inTauri } from "../lib/windows";
import { MockMeetingStore } from "./mock/store";
import { MockRecorderClient, TauriRecorderClient, type RecorderClient } from "./recorder";
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

const mockSettings = real ? undefined : new MockSettingsStore();
export const settingsStore: SettingsStore = mockSettings ?? new TauriSettingsStore();
const mockMeetings = mockSettings ? new MockMeetingStore(new Date(), { empty, yourName: () => mockSettings.current().yourName }) : undefined;
export const meetingStore: MeetingStore = mockMeetings ?? new TauriMeetingStore();
/** Browser preview only: the mock recorder saves no meeting, so the guided test recording asks for a simulated one. */
export const simulateRecording = mockMeetings ? (title: string, durationMs: number, tags: string[]) => mockMeetings.addRecording(title, durationMs, tags) : undefined;
export const recorderClient: RecorderClient = real ? new TauriRecorderClient() : new MockRecorderClient();
export const usingRealStorage = real;
